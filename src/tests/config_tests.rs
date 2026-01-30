//! Tests for VM configuration structures.

use crate::config::{AxVCpuConfig, PhysCpuList, VMImageConfig};
use axaddrspace::GuestPhysAddr;

#[test]
fn test_ax_vcpu_config_default() {
    let config = AxVCpuConfig::default();
    assert_eq!(config.bsp_entry.as_usize(), 0);
    assert_eq!(config.ap_entry.as_usize(), 0);
}

#[test]
fn test_ax_vcpu_config_custom() {
    let config = AxVCpuConfig {
        bsp_entry: GuestPhysAddr::from(0x1000),
        ap_entry: GuestPhysAddr::from(0x2000),
    };
    assert_eq!(config.bsp_entry.as_usize(), 0x1000);
    assert_eq!(config.ap_entry.as_usize(), 0x2000);
}

#[test]
fn test_ax_vcpu_config_clone() {
    let config1 = AxVCpuConfig {
        bsp_entry: GuestPhysAddr::from(0x1000),
        ap_entry: GuestPhysAddr::from(0x2000),
    };
    let config2 = config1.clone();
    assert_eq!(config1.bsp_entry.as_usize(), config2.bsp_entry.as_usize());
    assert_eq!(config1.ap_entry.as_usize(), config2.ap_entry.as_usize());
}

#[test]
fn test_vm_image_config_default() {
    let config = VMImageConfig::default();
    assert_eq!(config.kernel_load_gpa.as_usize(), 0);
    assert!(config.bios_load_gpa.is_none());
    assert!(config.dtb_load_gpa.is_none());
    assert!(config.ramdisk_load_gpa.is_none());
}

#[test]
fn test_vm_image_config_with_all_options() {
    let config = VMImageConfig {
        kernel_load_gpa: GuestPhysAddr::from(0x80000000),
        bios_load_gpa: Some(GuestPhysAddr::from(0x0)),
        dtb_load_gpa: Some(GuestPhysAddr::from(0x81000000)),
        ramdisk_load_gpa: Some(GuestPhysAddr::from(0x82000000)),
    };

    assert_eq!(config.kernel_load_gpa.as_usize(), 0x80000000);
    assert_eq!(config.bios_load_gpa.unwrap().as_usize(), 0x0);
    assert_eq!(config.dtb_load_gpa.unwrap().as_usize(), 0x81000000);
    assert_eq!(config.ramdisk_load_gpa.unwrap().as_usize(), 0x82000000);
}

#[test]
fn test_vm_image_config_clone() {
    let config1 = VMImageConfig {
        kernel_load_gpa: GuestPhysAddr::from(0x80000000),
        bios_load_gpa: Some(GuestPhysAddr::from(0x0)),
        dtb_load_gpa: None,
        ramdisk_load_gpa: None,
    };
    let config2 = config1.clone();

    assert_eq!(
        config1.kernel_load_gpa.as_usize(),
        config2.kernel_load_gpa.as_usize()
    );
    assert_eq!(
        config1.bios_load_gpa.map(|a| a.as_usize()),
        config2.bios_load_gpa.map(|a| a.as_usize())
    );
}

#[test]
fn test_phys_cpu_list_default() {
    let list = PhysCpuList::default();
    assert_eq!(list.cpu_num(), 0);
    assert!(list.phys_cpu_ids().is_none());
    assert!(list.phys_cpu_sets().is_none());
}

#[test]
fn test_phys_cpu_list_get_vcpu_affinities_empty() {
    let list = PhysCpuList::default();
    let affinities = list.get_vcpu_affinities_pcpu_ids();
    assert!(affinities.is_empty());
}

#[test]
fn test_vm_image_config_debug() {
    use alloc::format;

    let config = VMImageConfig {
        kernel_load_gpa: GuestPhysAddr::from(0x80000000),
        bios_load_gpa: None,
        dtb_load_gpa: None,
        ramdisk_load_gpa: None,
    };

    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("VMImageConfig"));
    assert!(debug_str.contains("kernel_load_gpa"));
}

#[test]
fn test_ax_vcpu_config_debug() {
    use alloc::format;

    let config = AxVCpuConfig {
        bsp_entry: GuestPhysAddr::from(0x1000),
        ap_entry: GuestPhysAddr::from(0x2000),
    };

    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("AxVCpuConfig"));
}

#[test]
fn test_phys_cpu_list_debug() {
    use alloc::format;

    let list = PhysCpuList::default();
    let debug_str = format!("{:?}", list);
    assert!(debug_str.contains("PhysCpuList"));
}
