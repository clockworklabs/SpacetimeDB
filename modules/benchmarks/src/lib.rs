mod circles;
mod ia_loop;
mod synthetic;

pub(crate) struct Load {
    initial_load: u32,
    small_table: u32,
    num_players: u32,
    big_table: u32,
    biggest_table: u32,
}

impl Load {
    pub(crate) fn new(initial_load: u32) -> Self {
        Self {
            initial_load,
            small_table: initial_load * 10,
            num_players: initial_load * 100,
            big_table: initial_load * 10_000,
            biggest_table: initial_load * 20_000,
        }
    }
}
