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
            small_table: initial_load,
            num_players: initial_load,
            big_table: initial_load * 50,
            biggest_table: initial_load * 100,
        }
    }
}
