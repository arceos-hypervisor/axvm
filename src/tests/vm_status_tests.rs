//! Tests for VMStatus enumeration.

use crate::vm::VMStatus;

#[test]
fn test_vm_status_as_str() {
    assert_eq!(VMStatus::Loading.as_str(), "loading");
    assert_eq!(VMStatus::Loaded.as_str(), "loaded");
    assert_eq!(VMStatus::Running.as_str(), "running");
    assert_eq!(VMStatus::Suspended.as_str(), "suspended");
    assert_eq!(VMStatus::Stopping.as_str(), "stopping");
    assert_eq!(VMStatus::Stopped.as_str(), "stopped");
}

#[test]
fn test_vm_status_as_str_with_icon() {
    assert!(VMStatus::Loading.as_str_with_icon().contains("loading"));
    assert!(VMStatus::Loaded.as_str_with_icon().contains("loaded"));
    assert!(VMStatus::Running.as_str_with_icon().contains("running"));
    assert!(VMStatus::Suspended.as_str_with_icon().contains("suspended"));
    assert!(VMStatus::Stopping.as_str_with_icon().contains("stopping"));
    assert!(VMStatus::Stopped.as_str_with_icon().contains("stopped"));
}

#[test]
fn test_vm_status_display() {
    use alloc::format;

    assert_eq!(format!("{}", VMStatus::Loading), "loading");
    assert_eq!(format!("{}", VMStatus::Loaded), "loaded");
    assert_eq!(format!("{}", VMStatus::Running), "running");
    assert_eq!(format!("{}", VMStatus::Suspended), "suspended");
    assert_eq!(format!("{}", VMStatus::Stopping), "stopping");
    assert_eq!(format!("{}", VMStatus::Stopped), "stopped");
}

#[test]
fn test_vm_status_debug() {
    use alloc::format;

    let debug_str = format!("{:?}", VMStatus::Running);
    assert!(debug_str.contains("Running"));
}

#[test]
fn test_vm_status_clone() {
    let status1 = VMStatus::Running;
    let status2 = status1.clone();
    assert_eq!(status1, status2);
}

#[test]
fn test_vm_status_copy() {
    let status1 = VMStatus::Suspended;
    let status2 = status1; // Copy
    assert_eq!(status1, status2);
}

#[test]
fn test_vm_status_eq() {
    assert_eq!(VMStatus::Loading, VMStatus::Loading);
    assert_eq!(VMStatus::Running, VMStatus::Running);
    assert_ne!(VMStatus::Loading, VMStatus::Running);
    assert_ne!(VMStatus::Stopped, VMStatus::Stopping);
}

#[test]
fn test_vm_status_all_variants() {
    // Test that all variants can be created and are distinct
    let statuses = [
        VMStatus::Loading,
        VMStatus::Loaded,
        VMStatus::Running,
        VMStatus::Suspended,
        VMStatus::Stopping,
        VMStatus::Stopped,
    ];

    // Each status should have a unique string representation
    let mut seen = alloc::collections::BTreeSet::new();
    for status in &statuses {
        let s = status.as_str();
        assert!(seen.insert(s), "Duplicate status string: {}", s);
    }
}
