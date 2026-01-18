use axaddrspace::{HostPhysAddr, HostVirtAddr};
use axerrno::AxResult;
use memory_addr::{PhysAddr, VirtAddr};
use page_table_multiarch::PagingHandler;

/// The interfaces which the underlying software (kernel or hypervisor) must implement.
pub trait AxVMHal: Sized {
    /// The low-level **OS-dependent** helpers that must be provided for physical address management.
    type PagingHandler: PagingHandler;

    /// Converts a virtual address to the corresponding physical address.
    fn virt_to_phys(vaddr: HostVirtAddr) -> HostPhysAddr;

    /// Current time in nanoseconds.
    fn current_time_nanos() -> u64;

    /// Current VM ID.
    fn current_vm_id() -> usize;

    /// Current Virtual CPU ID.
    fn current_vcpu_id() -> usize;

    /// Current Physical CPU ID.
    fn current_pcpu_id() -> usize;

    /// Get the Physical CPU ID where the specified VCPU of the current VM resides.
    ///
    /// Returns an error if the VCPU is not found.
    fn vcpu_resides_on(vm_id: usize, vcpu_id: usize) -> AxResult<usize>;

    /// Inject an IRQ to the specified VCPU.
    ///
    /// This method should find the physical CPU where the specified VCPU resides and inject the IRQ
    /// to it on that physical CPU with [`axvcpu::AxVCpu::inject_interrupt`].
    ///
    /// Returns an error if the VCPU is not found.
    fn inject_irq_to_vcpu(vm_id: usize, vcpu_id: usize, irq: usize) -> AxResult;
}

pub struct PagingHandlerImpl;

impl PagingHandler for PagingHandlerImpl {
    fn alloc_frame() -> Option<PhysAddr> {
        axvisor_api::memory::alloc_frame()
    }

    fn dealloc_frame(paddr: PhysAddr) {
        axvisor_api::memory::dealloc_frame(paddr)
    }

    fn phys_to_virt(paddr: PhysAddr) -> VirtAddr {
        axvisor_api::memory::phys_to_virt(paddr)
    }
}
