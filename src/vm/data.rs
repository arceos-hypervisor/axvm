use core::{alloc::Layout, ops::Range};
use std::{
    sync::{Arc, Mutex, Weak},
    vec::Vec,
};

pub use axaddrspace::MappingFlags;
use memory_addr::MemoryAddr;

use crate::{
    AxVMConfig, GuestPhysAddr, HostPhysAddr, HostVirtAddr,
    config::MemoryKind,
    vhal::{phys_to_virt, virt_to_phys},
    vm::addrspace::{VmRegion, VmRegionKind},
};
use crate::{vhal::ArchHal, vm::addrspace::VmRegionMap};

const ALIGN: usize = 1024 * 1024 * 2;

use super::addrspace::AddrSpace;

pub type VmDataArc = Arc<VmRunCommonData>;

pub struct VmRunCommonData {
    shared: Mutex<SharedData>,
    pub(crate) addrspace: Mutex<AddrSpace>,
}

impl VmRunCommonData {
    pub fn new(
        gpt_levels: usize,
        vm_addr_space: Range<GuestPhysAddr>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut memory_map = VmRegionMap::new(Vec::new());
        let vm_space_size = vm_addr_space.end.as_usize() - vm_addr_space.start.as_usize();
        memory_map.add(VmRegion {
            gpa: vm_addr_space.start,
            size: vm_space_size,
            kind: VmRegionKind::Passthrough,
        })?;

        // Create address space for the VM
        let address_space = AddrSpace::new_empty(
            gpt_levels,
            vm_addr_space.start.as_usize().into(),
            vm_space_size,
        )
        .map_err(|e| anyhow!("Failed to create address space: {e:?}"))?;
        Ok(Arc::new(Self {
            addrspace: Mutex::new(address_space),
            shared: Mutex::new(SharedData {
                memory_map,
                ..Default::default()
            }),
        }))
    }

    pub fn add_memory(&self, m: GuestMemory) {
        let mut s = self.shared.lock();
        s.memories.push(m);
    }

    pub fn add_reserved_memory(&self, r: GuestMemory) {
        self.shared.lock().reserved_memories.push(r);
    }

    pub(crate) fn memory_map_add_region(&self, region: VmRegion) -> anyhow::Result<()> {
        let mut s = self.shared.lock();
        s.memory_map.add(region).unwrap();
        Ok(())
    }

    pub fn new_memory(self: &Arc<Self>, kind: &MemoryKind, flags: MappingFlags) -> GuestMemory {
        let _gpa;
        let _size;
        let mut hva = HostVirtAddr::from(0);

        match kind {
            MemoryKind::Identical { size } => {
                hva = HostVirtAddr::from(unsafe {
                    alloc::alloc::alloc(Layout::from_size_align_unchecked(*size, ALIGN))
                } as usize);
                _gpa = GuestPhysAddr::from_usize(virt_to_phys(hva).as_usize());
                _size = *size;
                let mut g = self.addrspace.lock();
                g.map_linear(
                    _gpa.as_usize().into(),
                    hva.as_usize().into(),
                    _size.align_up_4k(),
                    flags,
                )
                .unwrap();
            }
            MemoryKind::Passthrough { hpa, size } => {
                hva = phys_to_virt(*hpa);
                _gpa = GuestPhysAddr::from_usize(hva.as_usize());
                _size = *size;
                let mut g = self.addrspace.lock();
                g.map_linear(
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
                let mut g = self.addrspace.lock();
                g.map_alloc(_gpa.as_usize().into(), _size.align_up_4k(), flags, true)
                    .unwrap();
            }
        }

        self.memory_map_add_region(VmRegion {
            gpa: _gpa,
            size: _size,
            kind: VmRegionKind::Memory,
        })
        .unwrap();

        GuestMemory {
            gpa: _gpa,
            hva,
            size: _size,
            kind: kind.clone(),
            owner: self.weak(),
        }
    }

    pub fn weak(self: &Arc<Self>) -> VmDataWeak {
        Arc::downgrade(self).into()
    }

    pub fn load_kernel_image(&mut self, config: &AxVMConfig) -> anyhow::Result<()> {
        let mut idx = 0;
        let image_cfg = config.image_config();
        let mut s = self.shared.lock();
        let gpa = if let Some(gpa) = image_cfg.kernel.gpa {
            let mut found = false;
            for (i, region) in s.memories.iter().enumerate() {
                if (region.gpa..region.gpa + region.size).contains(&gpa) {
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
            for (i, region) in s.memories.iter().enumerate() {
                if region.size >= image_cfg.kernel.data.len() {
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
        let offset = gpa.as_usize() - s.memories[idx].gpa().as_usize();
        s.memories[idx].copy_from_slice(offset, &image_cfg.kernel.data);
        s.kernel_region_index = idx;
        s.kernel_entry = gpa;
        Ok(())
    }

    pub fn gpt_root(&self) -> HostPhysAddr {
        let g = self.addrspace.lock();
        g.page_table_root().as_usize().into()
    }

    pub fn kernel_entry(&self) -> GuestPhysAddr {
        let s = self.shared.lock();
        s.kernel_entry
    }

    pub fn memories(&self) -> Vec<(GuestPhysAddr, usize)> {
        let s = self.shared.lock();
        s.memories.iter().map(|m| (m.gpa(), m.size())).collect()
    }

    pub fn reserved_memories(&self) -> Vec<(GuestPhysAddr, usize)> {
        let s = self.shared.lock();
        s.reserved_memories
            .iter()
            .map(|m| (m.gpa(), m.size()))
            .collect()
    }

    pub fn map_passthrough_regions(&self) -> anyhow::Result<()> {
        let s = self.shared.lock();
        let mut g = self.addrspace.lock();
        for region in s
            .memory_map
            .iter()
            .filter(|m| m.kind == VmRegionKind::Passthrough)
        {
            g.map_linear(
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
}

#[derive(Default)]
struct SharedData {
    memories: Vec<GuestMemory>,
    reserved_memories: Vec<GuestMemory>,
    kernel_region_index: usize,
    kernel_entry: GuestPhysAddr,
    memory_map: VmRegionMap,
}

impl SharedData {}

pub struct GuestMemory {
    gpa: GuestPhysAddr,
    hva: HostVirtAddr,
    size: usize,
    kind: MemoryKind,
    owner: VmDataWeak,
}

impl GuestMemory {
    pub fn copy_from_slice(&mut self, offset: usize, data: &[u8]) {
        assert!(data.len() <= self.size - offset);
        let owner = self.owner.try_use().unwrap();

        let g = owner.addrspace.lock();
        let hva = g
            .translated_byte_buffer(self.gpa.as_usize().into(), self.size)
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

    pub fn gpa(&self) -> GuestPhysAddr {
        self.gpa
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let owner = self.owner.try_use().unwrap();

        let mut result = vec![];
        let g = owner.addrspace.lock();
        let hva = g
            .translated_byte_buffer(self.gpa.as_usize().into(), self.size)
            .expect("Failed to translate memory region");
        for buff in hva {
            result.extend_from_slice(buff);
        }
        result.resize(self.size, 0);
        result
    }
}

impl Drop for GuestMemory {
    fn drop(&mut self) {
        let owner = match self.owner.inner.upgrade() {
            Some(o) => o,
            None => return,
        };

        let mut g = owner.addrspace.lock();
        match &self.kind {
            MemoryKind::Identical { .. } => {
                unsafe {
                    alloc::alloc::dealloc(
                        HostVirtAddr::from(self.hva.as_usize()).as_mut_ptr(),
                        Layout::from_size_align(self.size, ALIGN).unwrap(),
                    )
                };
            }
            _ => {
                g.unmap(self.gpa.as_usize().into(), self.size.align_up_4k())
                    .unwrap();
            }
        }
    }
}
