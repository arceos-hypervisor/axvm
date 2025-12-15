use alloc::{string::String, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    os::arceos::{api::task::AxCpuMask, modules::axtask::set_current_affinity},
    string::ToString,
};

use arm_vcpu::Aarch64VCpuSetupConfig;
use fdt_edit::{Node, NodeRef, Property, RegInfo};
use memory_addr::{MemoryAddr, align_down_4k, align_up_4k};

mod init;

use crate::{
    GuestPhysAddr, RunError, TASK_STACK_SIZE, Vm, VmData, VmStatusInitOps, VmStatusRunningOps,
    VmStatusStoppingOps,
    arch::cpu::VCpu,
    config::{AxVMConfig, MemoryKind},
    vhal::cpu::CpuHardId,
    vm::{MappingFlags, VmId},
};

pub use init::VmInit;

const VM_ASPACE_BASE: usize = 0x0;
const VM_ASPACE_SIZE: usize = 0x7fff_ffff_f000;

impl VmStatusRunningOps for VmStatusRunning {
    type Stopping = VmStatusStopping;

    fn stop(self) -> Result<Self::Stopping, (anyhow::Error, Self)>
    where
        Self: Sized,
    {
        Ok(VmStatusStopping {})
    }

    fn do_work(&mut self) -> Result<(), RunError> {
        if self.vcpu_running_count.load(Ordering::SeqCst) == 0 {
            Err(RunError::Exit)
        } else {
            Ok(())
        }
    }
}

pub struct VmStatusStopping {}

impl VmStatusStoppingOps for VmStatusStopping {}

/// Data needed when VM is running
pub struct VmStatusRunning {
    vcpus: Vec<VCpu>,
    data: VmData,
    dtb_addr: GuestPhysAddr,
    vcpu_running_count: Arc<AtomicUsize>,
}

impl VmStatusRunning {
    pub(crate) fn new(data: VmData, vcpus: Vec<VCpu>) -> Self {
        Self {
            vcpus,
            data,
            dtb_addr: GuestPhysAddr::from_usize(0),
            vcpu_running_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn make_dtb(&mut self, config: &AxVMConfig) -> anyhow::Result<()> {
        let flags =
            MappingFlags::READ | MappingFlags::WRITE | MappingFlags::WRITE | MappingFlags::USER;

        if let Some(dtb_cfg) = &config.image_config().dtb {
            debug!(
                "Loading DTB image into GPA @{:#x} for VM {} ({})",
                dtb_cfg.gpa.unwrap_or(0.into()).as_usize(),
                config.id(),
                config.name()
            );
            let kind = if let Some(gpa) = dtb_cfg.gpa {
                MemoryKind::Vmem {
                    gpa: gpa.into(),
                    size: dtb_cfg.data.len(),
                }
            } else {
                MemoryKind::Identical {
                    size: dtb_cfg.data.len(),
                }
            };

            let mut guest_mem = self.data.new_memory(&kind, flags);

            self.dtb_addr = guest_mem.gpa();

            guest_mem.copy_from_slice(0, &dtb_cfg.data);
            self.data.add_reserved_memory(guest_mem);
        } else {
            debug!(
                "No dtb provided, generating new dtb for {} ({})",
                config.id(),
                config.name()
            );
            let mut fdt = crate::fdt::fdt_edit().expect("Need fdt");

            let mut rm_nodes = vec![];
            let vcpu_hard_ls = self.vcpus.iter().map(|v| v.id).collect::<Vec<_>>();
            for cpu in fdt.find_by_path("/cpus/cpu") {
                if let Some(id) = cpu.regs() {
                    let id = CpuHardId::new(id[0].address as usize);
                    if vcpu_hard_ls.contains(&id) {
                        continue;
                    }
                }

                rm_nodes.push(cpu.path());
            }

            for path in rm_nodes {
                fdt.remove_node(&path).unwrap();
            }

            let nodes = fdt
                .find_by_path("/memory")
                .into_iter()
                .map(|o| o.path())
                .collect::<Vec<_>>();
            for path in nodes {
                let _ = fdt.remove_node(&path);
            }

            let root_address_cells = fdt.root().address_cells().unwrap_or(2);
            let root_size_cells = fdt.root().size_cells().unwrap_or(2);

            for (i, m) in self.data.memories().iter().enumerate() {
                let mut node = Node::new(&format!("memory@{i}"));
                let mut prop = Property::new("device_type", vec![]);
                prop.set_string("memory");
                node.add_property(prop);
                fdt.root_mut().add_child(node);
                let mut node = fdt
                    .get_by_path_mut(&format!("/memory@{i}"))
                    .expect("must has node");
                node.set_regs(&[RegInfo {
                    address: m.0.as_usize() as u64,
                    size: Some(m.1 as u64),
                }]);
            }

            let dtb_data = fdt.encode();

            let f = fdt_edit::Fdt::from_bytes(&dtb_data).unwrap();
            debug!("Generated DTB:\n{f}");

            let mut guest_mem = self.data.memories().into_iter().next().unwrap();
            let mut dtb_start =
                (guest_mem.0.as_usize() + guest_mem.1.min(512 * 1024 * 1024)) - dtb_data.len();
            dtb_start = dtb_start.align_down_4k();

            self.dtb_addr = GuestPhysAddr::from(dtb_start);
            debug!(
                "Loading generated DTB into GPA @{:#x} for VM {} ({})",
                dtb_start,
                config.id(),
                config.name()
            );
            self.copy_to_guest(self.dtb_addr, &dtb_data);
        }

        Ok(())
    }

    fn handle_node_regs(dev_vec: &mut [DevMapConfig], node: &NodeRef<'_>) {}

    fn copy_to_guest(&mut self, gpa: GuestPhysAddr, data: &[u8]) {
        let parts = self
            .data
            .addrspace
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

/// Information about a device in the VM
#[derive(Debug, Clone)]
pub struct DeviceInfo {}

#[derive(Debug, Clone)]
struct DevMapConfig {
    gpa: GuestPhysAddr,
    size: usize,
    name: String,
}
