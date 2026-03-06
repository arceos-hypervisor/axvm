// Copyright 2025 The Axvisor Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use axaddrspace::{HostPhysAddr, HostVirtAddr};
use axerrno::AxResult;
use memory_addr::{PAGE_SIZE_4K, PhysAddr, VirtAddr};
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
    fn alloc_frames(num: usize, align: usize) -> Option<PhysAddr> {
        let align_frames = if align.is_multiple_of(PAGE_SIZE_4K) {
            align / PAGE_SIZE_4K
        } else {
            panic!("align must be multiple of PAGE_SIZE_4K")
        };

        let align_frames_pow2 = if align_frames.is_power_of_two() {
            align_frames.trailing_zeros()
        } else {
            panic!("align must be a power of 2")
        };

        axvisor_api::memory::alloc_contiguous_frames(num, align_frames_pow2 as _)
    }

    fn dealloc_frames(paddr: PhysAddr, num: usize) {
        axvisor_api::memory::dealloc_contiguous_frames(paddr, num);
    }

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
