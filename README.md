[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/arceos-hypervisor/axvm)

# AxVM

The axvm crate provides a minimal virtual machine monitor (VMM) for the ArceOS hypervisor ecosystem. It implements core virtualization capabilities including virtual CPU management, memory virtualization, and device emulation across multiple hardware architectures. 

# APIS

``````
impl<H: AxVMHal, U: AxVCpuHal> AxVM<H, U> {
    /// Creates a new VM with the given configuration.
    /// Returns an error if the configuration is invalid.
    /// The VM is not started until `boot` is called.
    pub fn new(config: AxVMConfig) -> AxResult<AxVMRef<H, U>>
}
``````

```
// Boots the VM by setting the running flag as true.
boot(&self) -> AxResult
```

```
// Run a vCPU according to the given vcpu_id.
run_vcpu(&self, vcpu_id: usize) -> AxResult<AxVCpuExitReason>
```

```
// Returns if the VM is shutting down.
shutting_down(&self) -> bool
```

``````
// Returns guest VM image load region in `Vec<&'static mut [u8]>`
get_image_load_region(
        &self,
        image_load_gpa: GuestPhysAddr,
        image_size: usize,
    ) -> AxResult<Vec<&'static mut [u8]>> 
``````

``````
// Returns the base address of the two-stage address translation page table for the VM.
ept_root(&self) -> HostPhysAddr
``````

# Examples

### Implementation for `AxVMHal` trait.

`````` 
use std::os::arceos;
use memory_addr::{PAGE_SIZE_4K, align_up_4k};
use page_table_multiarch::PagingHandler;
use arceos::modules::{axalloc, axhal};
use axaddrspace::{HostPhysAddr, HostVirtAddr};
use axvcpu::AxVCpuHal;
use axvm::{AxVMHal, AxVMPerCpu};

pub struct AxVMHalImpl;

impl AxVMHal for AxVMHalImpl {
    type PagingHandler = axhal::paging::PagingHandlerImpl;

    fn alloc_memory_region_at(base: HostPhysAddr, size: usize) -> bool {
        axalloc::global_allocator()
            .alloc_pages_at(
                base.as_usize(),
                align_up_4k(size) / PAGE_SIZE_4K,
                PAGE_SIZE_4K,
            )
            .map_err(|err| {
                error!(
                    "Failed to allocate memory region [{:?}~{:?}]: {:?}",
                    base,
                    base + size,
                    err
                );
            })
            .is_ok()
    }

    fn dealloc_memory_region_at(base: HostPhysAddr, size: usize) {
        axalloc::global_allocator().dealloc_pages(base.as_usize(), size / PAGE_SIZE_4K)
    }

    fn virt_to_phys(vaddr: HostVirtAddr) -> HostPhysAddr {
        axhal::mem::virt_to_phys(vaddr)
    }

    fn current_time_nanos() -> u64 {
        axhal::time::monotonic_time_nanos()
    }
}

``````

### Init VM

``````
let vm_create_config =
            AxVMCrateConfig::from_toml(raw_cfg_str).expect("Failed to resolve VM config");
let vm_config = AxVMConfig::from(vm_create_config.clone());
// Create VM.
let vm = VM::new(vm_config).expect("Failed to create VM");

// Load corresponding images for VM.
load_vm_images(vm_create_config, vm.clone()).expect("Failed to load VM images");
// Sets up the primary vCPU for the given VM,
// generally the first vCPU in the vCPU list,
// and initializing their respective wait queues and task lists.
// VM's secondary vCPUs are not started at this point.
vcpus::setup_vm_primary_vcpu(vm);
``````

### Start VM

``````
match vm.boot() {
            Ok(_) => {
                vcpus::notify_primary_vcpu(vm.id());
                RUNNING_VM_COUNT.fetch_add(1, Ordering::Release);
                info!("VM[{}] boot success", vm.id())
            }
            Err(err) => warn!("VM[{}] boot failed, error {:?}", vm.id(), err),
        }
``````

[More detailed usage](https://github.com/arceos-hypervisor/axvisor)

Virtual Machine **resource management** crate for [`ArceOS`](https://github.com/arceos-org/arceos)'s hypervisor variant.

* resources:
  * vcpu: [axvcpu](https://github.com/arceos-hypervisor/axvcpu) list
  * memory: [axaddrspace](https://github.com/arceos-hypervisor/axaddrspace) for guest memory management
  * device: [axdevice](https://github.com/arceos-hypervisor/axdevice) list