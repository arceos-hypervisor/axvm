use std::{
    collections::{btree_map::BTreeMap, btree_map::Entry, btree_set::BTreeSet},
    string::{String, ToString},
    vec::Vec,
};

use anyhow::Result;
use fdt_parser::{Fdt, Node};
use vm_fdt::{FdtWriter, FdtWriterNode};

use crate::{AxVMConfig, GuestPhysAddr, fdt::fdt, vhal::cpu::CpuHardId};

pub struct FdtBuilder {
    pub cpu_hard_ids: Vec<CpuHardId>,
    pub memories: Vec<(GuestPhysAddr, usize)>, // (start, size)
}

impl FdtBuilder {
    pub fn generate(&self, _vm_cfg: &AxVMConfig) -> Result<Vec<u8>> {
        let mut generator = Gen::new();
        generator.generate()
    }

    // pub fn generate2(&self, vm_cfg: &AxVMConfig) -> anyhow::Result<Vec<u8>> {
    //     let mut fdt_writer = FdtWriter::new().unwrap();
    //     // Track the level of the previously processed node for level change handling
    //     let mut previous_node_level = 0;
    //     // Maintain a stack of FDT nodes to correctly start and end nodes
    //     let mut node_stack: Vec<FdtWriterNode> = Vec::new();
    //     let fdt = super::fdt().ok_or_else(|| anyhow!("No FDT found"))?;

    //     let passthrough_device_names = find_all_passthrough_devices(vm_cfg, &fdt);

    //     let all_nodes = fdt.all_nodes();

    //     for (index, node) in all_nodes.iter().enumerate() {
    //         let node_path = build_node_path(&all_nodes, index);
    //         let node_action = determine_node_action(node, &node_path, &passthrough_device_names);

    //         match node_action {
    //             NodeAction::RootNode => {
    //                 node_stack.push(fdt_writer.begin_node("").unwrap());
    //             }
    //             NodeAction::CpuNode => {
    //                 let need = need_cpu_node(&self.cpu_hard_ids, node, &node_path);
    //                 if need {
    //                     handle_node_level_change(
    //                         &mut fdt_writer,
    //                         &mut node_stack,
    //                         node.level(),
    //                         previous_node_level,
    //                     );
    //                     node_stack.push(fdt_writer.begin_node(node.name()).unwrap());
    //                 } else {
    //                     continue;
    //                 }
    //             }
    //             NodeAction::Skip => {
    //                 continue;
    //             }
    //             _ => {
    //                 trace!(
    //                     "Found exact passthrough device node: {}, path: {}",
    //                     node.name(),
    //                     node_path
    //                 );
    //                 handle_node_level_change(
    //                     &mut fdt_writer,
    //                     &mut node_stack,
    //                     node.level(),
    //                     previous_node_level,
    //                 );
    //                 node_stack.push(fdt_writer.begin_node(node.name()).unwrap());
    //             }
    //         }

    //         previous_node_level = node.level();

    //         // Copy all properties of the node
    //         for prop in node.properties() {
    //             fdt_writer.property(prop.name, prop.raw_value()).unwrap();
    //         }
    //     }

    //     // End all unclosed nodes
    //     while let Some(node) = node_stack.pop() {
    //         previous_node_level -= 1;
    //         fdt_writer.end_node(node).unwrap();
    //     }
    //     assert_eq!(previous_node_level, 0);

    //     let out = fdt_writer.finish().unwrap();

    //     Ok(out)
    // }
}

struct Gen {
    tree: Tree,
}

impl Gen {
    fn new() -> Self {
        Self {
            tree: Tree::default(),
        }
    }

    fn generate(&mut self) -> Result<Vec<u8>> {
        let fdt = fdt().ok_or_else(|| anyhow::anyhow!("No FDT found"))?;
        let all_nodes = fdt.all_nodes();

        for (index, node) in all_nodes.iter().enumerate() {
            let path = build_node_path(&all_nodes, index);
            self.tree.insert(&path, node.clone())?;
        }

        self.tree.finalize()?;
        self.to_data()
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut fdt_writer = FdtWriter::new().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.tree.write(&mut fdt_writer)?;
        let data = fdt_writer.finish().map_err(|e| anyhow::anyhow!("{e}"))?;

        let fdt = Fdt::from_bytes(&data)?;
        print_fdt(&fdt);
        Ok(data)
    }
}

#[derive(Default)]
struct Tree {
    nodes: BTreeMap<String, TreeNode>,
    pending_links: Vec<(String, String)>,
}

impl Tree {
    fn insert(&mut self, path: &str, node: Node) -> Result<()> {
        match self.nodes.entry(path.to_string()) {
            Entry::Occupied(mut occ) => occ.get_mut().node = node,
            Entry::Vacant(vac) => {
                vac.insert(TreeNode::new(node));
            }
        }

        if let Some(parent) = parent_path(path) {
            self.pending_links.push((parent, path.to_string()));
        }

        Ok(())
    }

    fn finalize(&mut self) -> Result<()> {
        for (parent, child) in self.pending_links.drain(..) {
            let parent_node = self
                .nodes
                .get_mut(&parent)
                .ok_or_else(|| anyhow::anyhow!("Parent node {parent} missing for {child}"))?;
            parent_node.children.push(child);
        }
        Ok(())
    }

    fn write(&self, writer: &mut FdtWriter) -> Result<()> {
        self.write_node(writer, "/")
    }

    fn write_node(&self, writer: &mut FdtWriter, path: &str) -> Result<()> {
        let entry = self
            .nodes
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Node {path} not found"))?;
        debug!("Writing node: {}", path);
        let name = if path == "/" { "" } else { entry.node.name() };
        let handle = writer
            .begin_node(name)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        for prop in entry.node.properties() {
            writer
                .property(prop.name, prop.raw_value())
                .map_err(|e| anyhow::anyhow!("{e}"))?;
        }

        for child in &entry.children {
            self.write_node(writer, child)?;
        }

        writer.end_node(handle).map_err(|e| anyhow::anyhow!("{e}"))
    }
}

struct TreeNode {
    node: Node,
    children: Vec<String>,
}

impl TreeNode {
    fn new(node: Node) -> Self {
        Self {
            node,
            children: Vec::new(),
        }
    }
}

fn parent_path(path: &str) -> Option<String> {
    if path == "/" {
        None
    } else if let Some(idx) = path.rfind('/') {
        if idx == 0 {
            Some("/".to_string())
        } else {
            Some(path[..idx].to_string())
        }
    } else {
        None
    }
}

/// Determine if CPU node is needed
fn need_cpu_node(phys_cpu_ids: &[CpuHardId], node: &Node, node_path: &str) -> bool {
    let mut should_include_node = false;

    if !node_path.starts_with("/cpus/cpu@") {
        should_include_node = true;
    } else if let Ok(mut cpu_reg) = node.reg()
        && let Some(reg_entry) = cpu_reg.first()
    {
        let cpu_address = reg_entry.address as usize;
        debug!(
            "Checking CPU node {} with address 0x{:x}",
            node.name(),
            cpu_address
        );
        // Check if this CPU address is in the configured phys_cpu_ids
        if phys_cpu_ids.contains(&CpuHardId::new(cpu_address)) {
            should_include_node = true;
            debug!(
                "CPU node {} with address 0x{:x} is in phys_cpu_ids, including in guest FDT",
                node.name(),
                cpu_address
            );
        } else {
            debug!(
                "CPU node {} with address 0x{:x} is NOT in phys_cpu_ids, skipping",
                node.name(),
                cpu_address
            );
        }
    }
    should_include_node
}

/// Build the full path of a node based on node level relationships
/// Build the path by traversing all nodes and constructing paths based on level relationships to avoid path conflicts for nodes with the same name
pub fn build_node_path(all_nodes: &[Node], target_index: usize) -> String {
    let mut path_stack: Vec<String> = Vec::new();

    for node in all_nodes.iter().take(target_index + 1) {
        let level = node.level();

        if level == 1 {
            path_stack.clear();
            if node.name() != "/" {
                path_stack.push(node.name().to_string());
            }
        } else {
            while path_stack.len() >= level - 1 {
                path_stack.pop();
            }
            path_stack.push(node.name().to_string());
        }
    }

    // Build the full path of the current node
    if path_stack.is_empty() || (path_stack.len() == 1 && path_stack[0] == "/") {
        "/".to_string()
    } else {
        "/".to_string() + &path_stack.join("/")
    }
}

/// Determine node processing action
fn determine_node_action(
    node: &Node,
    node_path: &str,
    passthrough_device_names: &[String],
) -> NodeAction {
    if node.name() == "/" {
        // Special handling for root node
        NodeAction::RootNode
    } else if node.name().starts_with("memory") {
        // Skip memory nodes, will add them later
        NodeAction::Skip
    } else if node_path.starts_with("/cpus") {
        NodeAction::CpuNode
    } else if passthrough_device_names.contains(&node_path.to_string()) {
        // Fully matched passthrough device node
        NodeAction::IncludeAsPassthroughDevice
    }
    // Check if the node is a descendant of a passthrough device (by path inclusion and level validation)
    else if is_descendant_of_passthrough_device(node_path, node.level(), passthrough_device_names)
    {
        NodeAction::IncludeAsChildNode
    }
    // Check if the node is an ancestor of a passthrough device (by path inclusion and level validation)
    else if is_ancestor_of_passthrough_device(node_path, passthrough_device_names) {
        NodeAction::IncludeAsAncestorNode
    } else {
        NodeAction::Skip
    }
}

/// Node processing action enumeration
enum NodeAction {
    /// Skip node, not included in guest FDT
    Skip,
    /// Root node
    RootNode,
    /// CPU node
    CpuNode,
    /// Include node as passthrough device node
    IncludeAsPassthroughDevice,
    /// Include node as child node of passthrough device
    IncludeAsChildNode,
    /// Include node as ancestor node of passthrough device
    IncludeAsAncestorNode,
}

/// Handle node level changes to ensure correct FDT structure
fn handle_node_level_change(
    fdt_writer: &mut FdtWriter,
    node_stack: &mut Vec<FdtWriterNode>,
    current_level: usize,
    previous_level: usize,
) {
    if current_level <= previous_level {
        for _ in current_level..=previous_level {
            if let Some(end_node) = node_stack.pop() {
                fdt_writer.end_node(end_node).unwrap();
            }
        }
    }
}

/// Determine if node is a descendant of passthrough device
/// When node path contains a path from passthrough_device_names and is longer than it, it is its descendant node
/// Also use node_level as validation condition
fn is_descendant_of_passthrough_device(
    node_path: &str,
    node_level: usize,
    passthrough_device_names: &[String],
) -> bool {
    for passthrough_path in passthrough_device_names {
        // Check if the current node is a descendant of a passthrough device
        if node_path.starts_with(passthrough_path) && node_path.len() > passthrough_path.len() {
            // Ensure it is a true descendant path (separated by /)
            if passthrough_path == "/" || node_path.chars().nth(passthrough_path.len()) == Some('/')
            {
                // Use level relationship for validation: the level of a descendant node should be higher than its parent
                // Note: The level of the root node is 1, its direct child node level is 2, and so on
                let expected_parent_level = passthrough_path.matches('/').count();
                let current_node_level = node_level;

                // If passthrough_path is the root node "/", then its child node level should be 2
                // Otherwise, the child node level should be higher than the parent node level
                if (passthrough_path == "/" && current_node_level >= 2)
                    || (passthrough_path != "/" && current_node_level > expected_parent_level)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Determine if node is an ancestor of passthrough device
fn is_ancestor_of_passthrough_device(node_path: &str, passthrough_device_names: &[String]) -> bool {
    for passthrough_path in passthrough_device_names {
        // Check if the current node is an ancestor of a passthrough device
        if passthrough_path.starts_with(node_path) && passthrough_path.len() > node_path.len() {
            // Ensure it is a true ancestor path (separated by /)
            let next_char = passthrough_path.chars().nth(node_path.len()).unwrap_or(' ');
            if next_char == '/' || node_path == "/" {
                return true;
            }
        }
    }
    false
}

/// Return the collection of all passthrough devices in the configuration file and newly added devices found
pub fn find_all_passthrough_devices(vm_cfg: &AxVMConfig, fdt: &Fdt) -> Vec<String> {
    let initial_device_count = vm_cfg.pass_through_devices().len();

    // Pre-build node cache, store all nodes by path to improve lookup performance
    let node_cache: BTreeMap<String, Vec<Node>> = build_optimized_node_cache(fdt);

    // Get the list of configured device names
    let initial_device_names: Vec<String> = vm_cfg
        .pass_through_devices()
        .iter()
        .map(|dev| dev.name.clone())
        .collect();

    // Phase 1: Discover descendant nodes of all passthrough devices in the configuration file
    // Build a set of configured devices, using BTreeSet to improve lookup efficiency
    let mut configured_device_names: BTreeSet<String> =
        initial_device_names.iter().cloned().collect();

    // Used to store newly discovered related device names
    let mut additional_device_names = Vec::new();

    // Phase 1: Process initial devices and their descendant nodes
    // Note: Directly use device paths instead of device names
    for device_name in &initial_device_names {
        // Get all descendant node paths for this device
        let descendant_paths = get_descendant_nodes_by_path(&node_cache, device_name);
        trace!(
            "Found {} descendant paths for {}",
            descendant_paths.len(),
            device_name
        );

        for descendant_path in descendant_paths {
            if !configured_device_names.contains(&descendant_path) {
                trace!("Found descendant device: {descendant_path}");
                configured_device_names.insert(descendant_path.clone());

                additional_device_names.push(descendant_path.clone());
            } else {
                trace!("Device already exists: {descendant_path}");
            }
        }
    }

    info!(
        "Phase 1 completed: Found {} new descendant device names",
        additional_device_names.len()
    );

    // Phase 2: Discover dependency nodes for all existing devices (including descendant devices)
    let mut dependency_device_names = Vec::new();
    // Use a work queue of device names, including initial devices and descendant device names
    let mut devices_to_process: Vec<String> = configured_device_names.iter().cloned().collect();
    let mut processed_devices: BTreeSet<String> = BTreeSet::new();

    // Build phandle mapping table
    let phandle_map = build_phandle_map(fdt);

    // Use work queue to recursively find all dependent devices
    while let Some(device_node_path) = devices_to_process.pop() {
        // Avoid processing the same device repeatedly
        if processed_devices.contains(&device_node_path) {
            continue;
        }
        processed_devices.insert(device_node_path.clone());

        trace!("Analyzing dependencies for device: {device_node_path}");

        // Find direct dependencies of the current device
        let dependencies = find_device_dependencies(&device_node_path, &phandle_map, &node_cache);
        trace!(
            "Found {} dependencies: {:?}",
            dependencies.len(),
            dependencies
        );
        for dep_node_name in dependencies {
            // Check if dependency is already in configuration
            if !configured_device_names.contains(&dep_node_name) {
                trace!("Found new dependency device: {dep_node_name}");
                dependency_device_names.push(dep_node_name.clone());

                // Add dependency device name to work queue to further find its dependencies
                devices_to_process.push(dep_node_name.clone());
                configured_device_names.insert(dep_node_name.clone());
            }
        }
    }

    info!(
        "Phase 2 completed: Found {} new dependency device names",
        dependency_device_names.len()
    );

    // Phase 3: Find all excluded devices and remove them from the list
    // Convert Vec<Vec<String>> to Vec<String>
    let excluded_device_path: Vec<String> = vm_cfg
        .excluded_devices()
        .iter()
        .flatten()
        .cloned()
        .collect();
    let mut all_excludes_devices = excluded_device_path.clone();
    let mut process_excludeds: BTreeSet<String> = excluded_device_path.iter().cloned().collect();

    for device_path in &excluded_device_path {
        // Get all descendant node paths for this device
        let descendant_paths = get_descendant_nodes_by_path(&node_cache, device_path);
        info!(
            "Found {} descendant paths for {}",
            descendant_paths.len(),
            device_path
        );

        for descendant_path in descendant_paths {
            if !process_excludeds.contains(&descendant_path) {
                trace!("Found descendant device: {descendant_path}");
                process_excludeds.insert(descendant_path.clone());

                all_excludes_devices.push(descendant_path.clone());
            } else {
                trace!("Device already exists: {descendant_path}");
            }
        }
    }
    info!("Found excluded devices: {all_excludes_devices:?}");

    // Merge all device name lists
    let mut all_device_names = initial_device_names.clone();
    all_device_names.extend(additional_device_names);
    all_device_names.extend(dependency_device_names);

    // Remove excluded devices from the final list
    if !all_excludes_devices.is_empty() {
        info!(
            "Removing {} excluded devices from the list",
            all_excludes_devices.len()
        );
        let excluded_set: BTreeSet<String> = all_excludes_devices.into_iter().collect();

        // Filter out excluded devices
        all_device_names.retain(|device_name| {
            let should_keep = !excluded_set.contains(device_name);
            if !should_keep {
                info!("Excluding device: {device_name}");
            }
            should_keep
        });
    }

    // Phase 4: remove root node from the list
    all_device_names.retain(|device_name| device_name != "/");

    let final_device_count = all_device_names.len();
    info!(
        "Passthrough devices analysis completed. Total devices: {} (added: {})",
        final_device_count,
        final_device_count - initial_device_count
    );

    // Print final device list
    for (i, device_name) in all_device_names.iter().enumerate() {
        trace!("Final passthrough device[{i}]: {device_name}");
    }

    all_device_names
}

/// Build a simplified node cache table, traverse all nodes once and group by full path
/// Use level relationships to directly build paths, avoiding path conflicts for nodes with the same name
pub fn build_optimized_node_cache<'a>(fdt: &'a Fdt) -> BTreeMap<String, Vec<Node>> {
    let mut node_cache: BTreeMap<String, Vec<Node>> = BTreeMap::new();

    let all_nodes = fdt.all_nodes();

    for (index, node) in all_nodes.iter().enumerate() {
        let node_path = build_node_path(&all_nodes, index);
        if let Some(existing_nodes) = node_cache.get(&node_path)
            && !existing_nodes.is_empty()
        {
            error!(
                "Duplicate node path found: {} for node '{}' at level {}, existing node: '{}'",
                node_path,
                node.name(),
                node.level(),
                existing_nodes[0].name()
            );
        }

        trace!(
            "Adding node to cache: {} (level: {}, index: {})",
            node_path,
            node.level(),
            index
        );
        node_cache.entry(node_path).or_default().push(node.clone());
    }

    debug!(
        "Built simplified node cache with {} unique device paths",
        node_cache.len()
    );
    node_cache
}

/// Build a mapping table from phandle to node information, optimized version using fdt-parser convenience methods
/// Use full path instead of node name
/// Use level relationships to directly build paths, avoiding path conflicts for nodes with the same name
fn build_phandle_map(fdt: &Fdt) -> BTreeMap<u32, (String, BTreeMap<String, u32>)> {
    let mut phandle_map = BTreeMap::new();

    let all_nodes = fdt.all_nodes();

    for (index, node) in all_nodes.iter().enumerate() {
        let node_path = build_node_path(&all_nodes, index);

        // Collect node properties
        let mut phandle = None;
        let mut cells_map = BTreeMap::new();
        for prop in node.properties() {
            match prop.name {
                "phandle" | "linux,phandle" => {
                    phandle = Some(prop.u32().unwrap());
                }
                "#address-cells"
                | "#size-cells"
                | "#clock-cells"
                | "#reset-cells"
                | "#gpio-cells"
                | "#interrupt-cells"
                | "#power-domain-cells"
                | "#thermal-sensor-cells"
                | "#phy-cells"
                | "#dma-cells"
                | "#sound-dai-cells"
                | "#mbox-cells"
                | "#pwm-cells"
                | "#iommu-cells" => {
                    cells_map.insert(prop.name.to_string(), prop.u32().unwrap());
                }
                _ => {}
            }
        }

        // If phandle is found, store it together with the node's full path
        if let Some(ph) = phandle {
            phandle_map.insert(ph, (node_path, cells_map));
        }
    }
    phandle_map
}

/// Parse properties containing phandle references intelligently based on #*-cells properties
/// Supports multiple formats:
/// - Single phandle: <phandle>
/// - phandle+specifier: <phandle specifier1 specifier2 ...>
/// - Multiple phandle references: <phandle1 spec1 spec2 phandle2 spec1 spec2 ...>
fn parse_phandle_property_with_cells(
    prop_data: &[u8],
    prop_name: &str,
    phandle_map: &BTreeMap<u32, (String, BTreeMap<String, u32>)>,
) -> Vec<(u32, Vec<u32>)> {
    let mut results = Vec::new();

    debug!(
        "Parsing property '{}' with cells info, data length: {} bytes",
        prop_name,
        prop_data.len()
    );

    if prop_data.is_empty() || prop_data.len() % 4 != 0 {
        warn!(
            "Property '{}' data length ({} bytes) is invalid",
            prop_name,
            prop_data.len()
        );
        return results;
    }

    let u32_values: Vec<u32> = prop_data
        .chunks(4)
        .map(|chunk| u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    let mut i = 0;
    while i < u32_values.len() {
        let potential_phandle = u32_values[i];

        // Check if it's a valid phandle
        if let Some((device_name, cells_info)) = phandle_map.get(&potential_phandle) {
            // Determine the number of cells required based on property name
            let cells_count = get_cells_count_for_property(prop_name, cells_info);
            trace!(
                "Property '{prop_name}' requires {cells_count} cells for device '{device_name}'"
            );

            // Check if there's enough data
            if i + cells_count < u32_values.len() {
                let specifiers: Vec<u32> = u32_values[i + 1..=i + cells_count].to_vec();
                debug!(
                    "Parsed phandle reference: phandle={potential_phandle:#x}, specifiers={specifiers:?}"
                );
                results.push((potential_phandle, specifiers));
                i += cells_count + 1; // Skip phandle and all specifiers
            } else {
                warn!(
                    "Property:{} not enough data for phandle {:#x}, expected {} cells but only {} values remaining",
                    prop_name,
                    potential_phandle,
                    cells_count,
                    u32_values.len() - i - 1
                );
                break;
            }
        } else {
            // If not a valid phandle, skip this value
            i += 1;
        }
    }

    results
}

/// Determine the required number of cells based on property name and target node's cells information
fn get_cells_count_for_property(prop_name: &str, cells_info: &BTreeMap<String, u32>) -> usize {
    let cells_property = match prop_name {
        "clocks" | "assigned-clocks" => "#clock-cells",
        "resets" => "#reset-cells",
        "power-domains" => "#power-domain-cells",
        "phys" => "#phy-cells",
        "interrupts" | "interrupts-extended" => "#interrupt-cells",
        "gpios" => "#gpio-cells",
        _ if prop_name.ends_with("-gpios") || prop_name.ends_with("-gpio") => "#gpio-cells",
        "dmas" => "#dma-cells",
        "thermal-sensors" => "#thermal-sensor-cells",
        "sound-dai" => "#sound-dai-cells",
        "mboxes" => "#mbox-cells",
        "pwms" => "#pwm-cells",
        _ => {
            debug!("Unknown property '{prop_name}', defaulting to 0 cell");
            return 0;
        }
    };

    cells_info.get(cells_property).copied().unwrap_or(0) as usize
}

/// Generic phandle property parsing function
/// Parse phandle references according to cells information with correct block size
/// Support single phandle and multiple phandle+specifier formats
/// Return full path instead of node name
fn parse_phandle_property(
    prop_data: &[u8],
    prop_name: &str,
    phandle_map: &BTreeMap<u32, (String, BTreeMap<String, u32>)>,
) -> Vec<String> {
    let mut dependencies = Vec::new();

    let phandle_refs = parse_phandle_property_with_cells(prop_data, prop_name, phandle_map);

    for (phandle, specifiers) in phandle_refs {
        if let Some((device_path, _cells_info)) = phandle_map.get(&phandle) {
            let spec_info = if !specifiers.is_empty() {
                format!(" (specifiers: {specifiers:?})")
            } else {
                String::new()
            };
            debug!(
                "Found {prop_name} dependency: phandle={phandle:#x}, device={device_path}{spec_info}"
            );
            dependencies.push(device_path.clone());
        }
    }

    dependencies
}

/// Device property classifier - used to identify properties that require special handling
struct DevicePropertyClassifier;

impl DevicePropertyClassifier {
    /// Phandle properties that require special handling - includes all properties that need dependency resolution
    const PHANDLE_PROPERTIES: &'static [&'static str] = &[
        "clocks",
        "power-domains",
        "phys",
        "resets",
        "dmas",
        "thermal-sensors",
        "mboxes",
        "assigned-clocks",
        "interrupt-parent",
        "phy-handle",
        "msi-parent",
        "memory-region",
        "syscon",
        "regmap",
        "iommus",
        "interconnects",
        "nvmem-cells",
        "sound-dai",
        "pinctrl-0",
        "pinctrl-1",
        "pinctrl-2",
        "pinctrl-3",
        "pinctrl-4",
    ];

    /// Determine if it's a phandle property that requires handling
    fn is_phandle_property(prop_name: &str) -> bool {
        Self::PHANDLE_PROPERTIES.contains(&prop_name)
            || prop_name.ends_with("-supply")
            || prop_name == "gpios"
            || prop_name.ends_with("-gpios")
            || prop_name.ends_with("-gpio")
            || (prop_name.contains("cells") && !prop_name.starts_with("#") && prop_name.len() >= 4)
    }
}

/// Find device dependencies
fn find_device_dependencies(
    device_node_path: &str,
    phandle_map: &BTreeMap<u32, (String, BTreeMap<String, u32>)>,
    node_cache: &BTreeMap<String, Vec<Node>>, // Add node_cache parameter
) -> Vec<String> {
    let mut dependencies = Vec::new();

    // Directly find nodes from node_cache, avoiding traversing all nodes
    if let Some(nodes) = node_cache.get(device_node_path) {
        // Traverse all properties of nodes to find dependencies
        for node in nodes {
            for prop in node.properties() {
                // Determine if it's a phandle property that needs to be processed
                if DevicePropertyClassifier::is_phandle_property(prop.name) {
                    let mut prop_deps =
                        parse_phandle_property(prop.raw_value(), prop.name, phandle_map);
                    dependencies.append(&mut prop_deps);
                }
            }
        }
    }

    dependencies
}

/// Get all descendant nodes based on parent node path (including child nodes, grandchild nodes, etc.)
/// Find all descendant nodes by looking up nodes with parent node path as prefix in node_cache
fn get_descendant_nodes_by_path<'a>(
    node_cache: &'a BTreeMap<String, Vec<Node>>,
    parent_path: &str,
) -> Vec<String> {
    let mut descendant_paths = Vec::new();

    // Special handling if parent path is root path
    let search_prefix = if parent_path == "/" {
        "/".to_string()
    } else {
        parent_path.to_string() + "/"
    };

    // Traverse node_cache, find all nodes with parent path as prefix
    for path in node_cache.keys() {
        // Check if path has parent path as prefix (and is not the parent path itself)
        if path.starts_with(&search_prefix) && path.len() > search_prefix.len() {
            // This is a descendant node path, add to results
            descendant_paths.push(path.clone());
        }
    }

    descendant_paths
}

fn print_fdt(fdt: &Fdt) {
    debug!("FDT Structure:");
    for node in fdt.all_nodes() {
        let indent = "  ".repeat(node.level().saturating_sub(1));
        debug!("{}Node: {}", indent, node.name());
        for prop in node.properties() {
            debug!(
                "{}  Property: {} = {:?}",
                indent,
                prop.name,
                prop.raw_value()
            );
        }
    }
}
