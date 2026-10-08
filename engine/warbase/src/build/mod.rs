//! Build domain: placement validation shared by ghost preview and commit
//! (same input => same verdict, warden UI3 lesson generalized). Two modes
//! from the map bundle — FreeGrid (loop-TD) and FixedSlots (warden).
//! Validation lives in the base; art and input live in the game.

use bevy::prelude::*;

/// Grid cell coordinates (FreeGrid mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell(pub i32, pub i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildReject {
    OutOfBounds,
    Blocked,
    Occupied,
    NoFreeSlot,
}

impl BuildReject {
    pub fn reason(&self) -> &'static str {
        match self {
            BuildReject::OutOfBounds => "out of bounds",
            BuildReject::Blocked => "blocked cell",
            BuildReject::Occupied => "occupied",
            BuildReject::NoFreeSlot => "no free slot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildMode {
    #[default]
    FreeGrid,
    FixedSlots,
}

/// Occupancy map. Ghost preview and commit both call `check` — the
/// same-source rule that killed warden's preview/commit drift.
#[derive(Resource, Debug, Default, Clone)]
pub struct BuildGrid {
    pub mode: BuildMode,
    pub cell_size: f32,
    pub cols: i32,
    pub rows: i32,
    /// World position of cell (0, 0)'s corner; lets the grid sit centered
    /// on maps with negative coordinates.
    pub origin: Vec2,
    /// Scenery/path-blocked cells (FreeGrid).
    pub blocked: std::collections::HashSet<Cell>,
    /// Fixed slot centers in world space (FixedSlots).
    pub slots: Vec<Vec2>,
    occupancy: std::collections::HashMap<Cell, Entity>,
}

impl BuildGrid {
    pub fn free_grid(cell_size: f32, cols: i32, rows: i32) -> Self {
        Self { mode: BuildMode::FreeGrid, cell_size, cols, rows, ..Default::default() }
    }

    pub fn fixed_slots(slots: Vec<Vec2>) -> Self {
        Self { mode: BuildMode::FixedSlots, slots, ..Default::default() }
    }

    pub fn world_to_cell(&self, world: Vec2) -> Cell {
        let local = world - self.origin;
        Cell(
            (local.x / self.cell_size).floor() as i32,
            (local.y / self.cell_size).floor() as i32,
        )
    }

    /// Same-source validation: the ghost preview calls exactly this.
    pub fn check(&self, cell: Cell) -> Result<(), BuildReject> {
        match self.mode {
            BuildMode::FreeGrid => {
                if cell.0 < 0 || cell.1 < 0 || cell.0 >= self.cols || cell.1 >= self.rows {
                    return Err(BuildReject::OutOfBounds);
                }
                if self.blocked.contains(&cell) {
                    return Err(BuildReject::Blocked);
                }
                if self.occupancy.contains_key(&cell) {
                    return Err(BuildReject::Occupied);
                }
                Ok(())
            }
            BuildMode::FixedSlots => {
                if self.occupancy.contains_key(&cell) {
                    Err(BuildReject::Occupied)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Commit a placement (caller re-checks `check` first).
    pub fn place(&mut self, cell: Cell, entity: Entity) {
        self.occupancy.insert(cell, entity);
    }

    /// Free a cell (selling/despawning).
    pub fn free(&mut self, cell: Cell) -> Option<Entity> {
        self.occupancy.remove(&cell)
    }

    /// First free fixed slot, if any.
    pub fn first_free_slot(&self) -> Result<(Cell, Vec2), BuildReject> {
        for (i, pos) in self.slots.iter().enumerate() {
            let cell = Cell(i as i32, 0);
            if !self.occupancy.contains_key(&cell) {
                return Ok((cell, *pos));
            }
        }
        Err(BuildReject::NoFreeSlot)
    }

    pub fn slot_center(&self, cell: Cell) -> Option<Vec2> {
        self.slots.get(cell.0 as usize).copied()
    }

    pub fn cell_center(&self, cell: Cell) -> Vec2 {
        self.origin
            + Vec2::new(
                (cell.0 as f32 + 0.5) * self.cell_size,
                (cell.1 as f32 + 0.5) * self.cell_size,
            )
    }

    pub fn used(&self) -> usize {
        self.occupancy.len()
    }
}
