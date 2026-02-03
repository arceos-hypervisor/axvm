use alloc::vec::Vec;
use axaddrspace::MappingFlags;
use axvdev::MmioRegion;
use core::{
    alloc::Layout,
    ops::{Deref, DerefMut, Range},
    ptr::NonNull,
};
use memory_addr::MemoryAddr;
use std::sync::{Arc, Mutex};

use ranges_ext::RangeInfo;

mod mmio;

pub(crate) use mmio::MmioRegions;

use crate::{
    AxVMConfig, GuestPhysAddr, HostPhysAddr, HostVirtAddr,
    config::MemoryKind,
    hal::{HalOp, phys_to_virt, virt_to_phys},
};

const ALIGN: usize = 1024 * 1024 * 2;

type AddrSpaceRaw = axaddrspace::AddrSpace<axhal::paging::PagingHandlerImpl>;
type AddrSpaceShared = Arc<Mutex<AddrSpaceRaw>>;

pub(crate) type VmRegionMap = ranges_ext::RangeSetAlloc<VmRegion>;

#[derive(Clone)]
pub struct VmAddrSpace(Arc<Mutex<Inner>>);

struct Inner {
    aspace: AddrSpaceShared,
    region_map: VmRegionMap,
    kernel_entry: GuestPhysAddr,
    kernel_memory_index: usize,
    memories: Vec<GuestMemory>,
    mmio: MmioRegions,
}

impl VmAddrSpace {
    pub fn new(gpt_levels: usize, vm_addr_space: Range<GuestPhysAddr>) -> anyhow::Result<Self> {
        let mut region_map = VmRegionMap::new(Vec::new());
        let vm_space_size = vm_addr_space.end.as_usize() - vm_addr_space.start.as_usize();
        region_map.add(VmRegion {
            gpa: vm_addr_space.start,
            size: vm_space_size,
            kind: VmRegionKind::Passthrough,
        })?;
        // Create address space for the VM
        let address_space = AddrSpaceRaw::new_empty(
            gpt_levels,
            vm_addr_space.start.as_usize().into(),
            vm_space_size,
        )
        .map_err(|e| anyhow!("Failed to create address space: {e:?}"))?;

        Ok(Self(Arc::new(Mutex::new(Inner {
            aspace: Arc::new(Mutex::new(address_space)),
            region_map,
            kernel_entry: GuestPhysAddr::from_usize(0),
            kernel_memory_index: 0,
            memories: vec![],
            mmio: MmioRegions::new(),
        }))))
    }

    pub fn gpt_root(&self) -> HostPhysAddr {
        let g = self.0.lock();
        g.aspace.lock().page_table_root().as_usize().into()
    }

    pub fn mmio_map(&self) -> MmioRegions {
        let g = self.0.lock();
        g.mmio.clone()
    }

    pub fn kernel_entry(&self) -> GuestPhysAddr {
        self.0.lock().kernel_entry
    }

    pub fn new_memory(&self, kind: &MemoryKind) -> anyhow::Result<()> {
        let mut g = self.0.lock();
        let _gpa;
        let _size;
        let _align = 0x1000;
        let mut hva = HostVirtAddr::from(0);
        let _payload;
        let flags =
            MappingFlags::READ | MappingFlags::WRITE | MappingFlags::EXECUTE | MappingFlags::USER;

        match kind {
            MemoryKind::Identical { size } => {
                let array = Array::new(*size, ALIGN);

                hva = HostVirtAddr::from(array.as_mut_ptr() as usize);
                _gpa = GuestPhysAddr::from_usize(virt_to_phys(hva).as_usize());
                _size = *size;
                _payload = Some(array);
                g.aspace
                    .lock()
                    .map_linear(
                        _gpa.as_usize().into(),
                        hva.as_usize().into(),
                        _size.align_up_4k(),
                        flags,
                    )
                    .unwrap();
            }
            MemoryKind::Reserved { hpa, size } => {
                hva = phys_to_virt(*hpa);
                _gpa = GuestPhysAddr::from_usize(hva.as_usize());
                _size = *size;
                _payload = None;
                g.aspace
                    .lock()
                    .map_linear(
                        _gpa.as_usize().into(),
                        hva.as_usize().into(),
                        _size.align_up_4k(),
                        flags,
                    )
                    .unwrap();
            }
            MemoryKind::Vmem { gpa, size } => {
                _gpa = *gpa;
                _size = *size;
                _payload = None;
                g.aspace
                    .lock()
                    .map_alloc(_gpa.as_usize().into(), _size.align_up_4k(), flags, true)
                    .unwrap();
            }
        }
        let aspace = g.aspace.clone();
        g.memories.push(GuestMemory {
            gpa: _gpa,
            hva,
            layout: Layout::from_size_align(_size, _align).unwrap(),
            _payload,
            aspace,
        });

        g.region_map.add(VmRegion {
            gpa: _gpa,
            size: _size,
            kind: VmRegionKind::Memory,
        })?;

        Ok(())
    }

    pub fn load_kernel_image(&self, config: &AxVMConfig) -> anyhow::Result<()> {
        let mut g = self.0.lock();

        let mut idx = 0;
        let image_cfg = config.image_config();
        let gpa = if let Some(gpa) = image_cfg.kernel.gpa {
            let mut found = false;
            for (i, region) in g.memories.iter().enumerate() {
                if (region.gpa..region.gpa + region.size()).contains(&gpa) {
                    idx = i;
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(anyhow!(
                    "Kernel load GPA {:#x} not within any memory region",
                    gpa.as_usize()
                ));
            }
            gpa
        } else {
            let mut gpa = None;
            for (i, region) in g.memories.iter().enumerate() {
                if region.size() >= image_cfg.kernel.data.len() {
                    gpa = Some(region.gpa + 2 * 1024 * 1024);
                    idx = i;
                    break;
                } else {
                    continue;
                }
            }
            gpa.ok_or(anyhow!("No suitable memory region found for kernel image"))?
        };

        debug!(
            "Loading kernel image into GPA @{:#x} for VM {} ({})",
            gpa.as_usize(),
            config.id(),
            config.name()
        );
        let offset = gpa.as_usize() - g.memories[idx].gpa().as_usize();

        g.memories[idx].copy_from_slice(offset, &image_cfg.kernel.data);
        g.kernel_memory_index = idx;
        g.kernel_entry = gpa;
        Ok(())
    }

    pub fn with_memories<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[GuestMemory]) -> R,
    {
        let g = self.0.lock();
        f(&g.memories)
    }

    pub fn load_dtb(&self, data: &[u8]) -> anyhow::Result<GuestPhysAddr> {
        let mut g = self.0.lock();
        let guest_mem = g.memories.iter().next().unwrap();
        let mut dtb_start =
            (guest_mem.gpa().as_usize() + guest_mem.size().min(512 * 1024 * 1024)) - data.len();
        dtb_start = dtb_start.align_down_4k();

        let gpa = GuestPhysAddr::from(dtb_start);
        debug!("Loading generated DTB into GPA @{:#x}", dtb_start,);
        g.copy_to_guest(gpa, &data);
        Ok(gpa)
    }

    pub fn map_passthrough_regions(&self) -> anyhow::Result<()> {
        let mut g = self.0.lock();

        for region in g
            .region_map
            .iter()
            .filter(|m| m.kind == VmRegionKind::Passthrough)
        {
            debug!(
                "mapping passthrough region: [{:#x}, {:#x})",
                region.gpa,
                region.gpa + region.size
            );

            g.aspace
                .lock()
                .map_linear(
                    region.gpa.as_usize().into(),
                    region.gpa.as_usize().into(),
                    region.size.align_up_4k(),
                    MappingFlags::READ
                        | MappingFlags::WRITE
                        | MappingFlags::EXECUTE
                        | MappingFlags::DEVICE
                        | MappingFlags::USER,
                )
                .map_err(|e| {
                    anyhow!(
                        "Failed to map passthrough region: [{:?}, {:?})\n {e:?}",
                        region.gpa,
                        region.gpa + region.size
                    )
                })?;
        }

        Ok(())
    }

    pub fn new_mmio(
        &self,
        dev_id: u32,
        gpa: Option<GuestPhysAddr>,
        size: usize,
    ) -> anyhow::Result<MmioRegion> {
        let mut g = self.0.lock();
        let array = Array::new(size, 0x1000);

        let hva = HostVirtAddr::from(array.as_mut_ptr() as usize);
        let gpa = gpa.unwrap();

        let hpa = virt_to_phys(hva);
        let aspace = g.aspace.clone();

        g.region_map.add(VmRegion {
            gpa,
            size,
            kind: VmRegionKind::Mmio,
        });

        g.mmio.add_region(GuestMmio::new(
            dev_id,
            GuestMemory {
                gpa,
                hva,
                layout: Layout::from_size_align(size, 0x1000).unwrap(),
                _payload: Some(array),
                aspace,
            },
        ));

        Ok(MmioRegion {
            addr: gpa.as_usize().into(),
            access: NonNull::new(hva.as_usize() as *mut u8).unwrap(),
            size,
        })
    }
}

impl Inner {
    fn copy_to_guest(&mut self, gpa: GuestPhysAddr, data: &[u8]) {
        let parts = self
            .aspace
            .lock()
            .translated_byte_buffer(gpa.as_usize().into(), data.len())
            .unwrap();
        let mut offset = 0;
        for part in parts {
            let len = part.len().min(data.len() - offset);
            part.copy_from_slice(&data[offset..offset + len]);
            offset += len;
        }
    }
}

#[derive(Debug, Clone)]
pub struct VmRegion {
    pub gpa: GuestPhysAddr,
    pub size: usize,
    pub kind: VmRegionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmRegionKind {
    Passthrough,
    Memory,
    Mmio,
}

impl RangeInfo for VmRegion {
    type Kind = VmRegionKind;

    type Type = GuestPhysAddr;

    fn range(&self) -> core::ops::Range<Self::Type> {
        self.gpa..GuestPhysAddr::from_usize(self.gpa.as_usize() + self.size)
    }

    fn kind(&self) -> &Self::Kind {
        &self.kind
    }

    fn overwritable(&self) -> bool {
        matches!(self.kind, VmRegionKind::Passthrough)
    }

    fn clone_with_range(&self, range: core::ops::Range<Self::Type>) -> Self {
        VmRegion {
            gpa: range.start,
            size: range.end.as_usize() - range.start.as_usize(),
            kind: self.kind,
        }
    }
}

pub struct Array {
    ptr: *mut u8,
    layout: Layout,
}

unsafe impl Send for Array {}
unsafe impl Sync for Array {}

impl Array {
    pub fn new(size: usize, align: usize) -> Self {
        let layout = Layout::from_size_align(size, align).unwrap();
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        Array { ptr, layout }
    }

    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr
    }
}

impl Deref for Array {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }
}

impl DerefMut for Array {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}

impl Drop for Array {
    fn drop(&mut self) {
        unsafe {
            alloc::alloc::dealloc(self.ptr, self.layout);
        }
    }
}

pub(crate) struct GuestMmio {
    pub(crate) dev_id: u32,
    m: GuestMemory,
}

impl GuestMmio {
    pub fn new(dev_id: u32, m: GuestMemory) -> Self {
        Self { dev_id, m }
    }
}

impl Deref for GuestMmio {
    type Target = GuestMemory;

    fn deref(&self) -> &Self::Target {
        &self.m
    }
}

impl DerefMut for GuestMmio {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.m
    }
}

pub struct GuestMemory {
    gpa: GuestPhysAddr,
    hva: HostVirtAddr,
    layout: Layout,
    aspace: AddrSpaceShared,
    _payload: Option<Array>,
}

impl GuestMemory {
    pub fn copy_from_slice(&mut self, offset: usize, data: &[u8]) {
        assert!(data.len() <= self.size() - offset);

        let g = self.aspace.lock();
        let hva = g
            .translated_byte_buffer(self.gpa.as_usize().into(), self.size())
            .expect("Failed to translate kernel image load address");
        let mut remain = data;
        let mut skip = offset;

        for buff in hva {
            if skip >= buff.len() {
                skip -= buff.len();
                continue;
            }
            let buff = &mut buff[skip..];
            skip = 0;

            let copy_size = core::cmp::min(remain.len(), buff.len());
            buff[..copy_size].copy_from_slice(&remain[..copy_size]);
            crate::arch::Hal::cache_flush(HostVirtAddr::from(buff.as_ptr() as usize), copy_size);
            remain = &remain[copy_size..];
            if remain.is_empty() {
                break;
            }
        }
    }

    pub fn hva(&self) -> HostVirtAddr {
        self.hva
    }

    pub fn gpa(&self) -> GuestPhysAddr {
        self.gpa
    }

    pub fn size(&self) -> usize {
        self.layout.size()
    }

    // pub fn to_vec(&self) -> Vec<u8> {
    //     let mut result = vec![];
    //     let g = self.aspace.lock();
    //     let hva = g
    //         .translated_byte_buffer(self.gpa.as_usize().into(), self.size())
    //         .expect("Failed to translate memory region");
    //     for buff in hva {
    //         result.extend_from_slice(buff);
    //     }
    //     result.resize(self.size(), 0);
    //     result
    // }
}

impl Drop for GuestMemory {
    fn drop(&mut self) {
        let start = self.gpa.as_usize().align_down(self.layout.align());
        let size = self.size().align_up(self.layout.align());

        self.aspace.lock().unmap(start.into(), size).unwrap();
    }
}
