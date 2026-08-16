//! UART discovery into one deduplicated candidate inventory.
//!
//! Later commits add discovery paths; the inventory merges their findings so
//! one physical UART is tested exactly once.

use crate::device::{Address, Inventory, Source};

/// Combines every discovery source into a deduplicated test inventory.
pub fn discover() -> Inventory {
    let mut inventory = Inventory::default();
    // COM1 is required wiring on the targeted machines, so it is always tested.
    inventory.add(Address::Port(0x3f8), None, Source::RequiredCom1);
    inventory
}
