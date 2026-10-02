use bevy::prelude::*;

/// Creative mode, as the `creative` console command sets it: the client sends it in every
/// command (flying, unhurt) and the Minecraft map reads it (blocks break at once, drop nothing
/// and never run out; middle click picks a block). It also carries `give` requests to the
/// Minecraft inventory, and the inventory's answers back to the console.
#[derive(Resource, Default)]
pub struct Creative {
    pub on: bool,
    /// `give <item> [count]`: the item as typed, and the count if one was given.
    pub gives: Vec<(String, Option<u32>)>,
    /// What became of them, for the console.
    pub replies: Vec<String>,
}
