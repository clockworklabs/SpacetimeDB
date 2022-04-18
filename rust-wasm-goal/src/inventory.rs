
pub struct Pocket {
    pub volume: i32,
    pub contents: std::option::Option<ItemStack>,
}

pub struct ItemStack {
    pub item_id: i32,
    pub quantity: i32,
}

pub struct Inventory {
    pub pockets: std::vec::Vec<Pocket>,
}

impl Inventory {
    pub fn add(&mut self, item_stack: ItemStack) -> bool {
        let mut item_stack_copy = item_stack.clone();
        let mut inventory_copy = self.clone();
        inventory_copy.add_partial(&mut item_stack_copy);
        if item_stack_copy.quantity == 0 {
            self.pockets = inventory_copy.pockets;
            return true;
        }
        false
    }

    pub fn add_multiple(&mut self, item_stacks: &Vec<ItemStack>) -> bool {
        let mut inventory_copy = self.clone();
        for &stack in item_stacks {
            if !inventory_copy.add(stack) {
                return false;
            }
        }
        self.pockets = inventory_copy.pockets;
        true
    }

    pub fn add_multiple_partial(&mut self, item_stacks: &mut Vec<ItemStack>) {
        for stack in item_stacks {
            self.add_partial(stack);
        }
    }

    pub fn add_partial(&mut self, item_stack: &mut ItemStack) {
        if item_stack.quantity <= 0 {
            return;
        }

        let volume: i32;

        let item_id = item_stack.item_id;
        if self.holds_cargo {
            volume = match StaticDataIndex::shared().cargos.by_id(item_id) {
                Some(c) => c.volume,
                None => return, // invalid cargo id, probably 0.
            };
        } else {
            volume = match StaticDataIndex::shared().items.by_id(item_id) {
                Some(i) => i.volume,
                None => return, // invalid item id, probably 0.
            }
        }

        // Fill partial pockets first
        for p in &mut self.pockets {
            let content = match &p.contents {
                Some(c) => c,
                None => continue,
            };
            if content.item_id == item_id {
                let inserted_count = std::cmp::min(p.can_fit_quantity(volume, self.holds_cargo), item_stack.quantity);
                p.add_quantity(inserted_count);
                item_stack.quantity -= inserted_count;
                if item_stack.quantity == 0 {
                    break;
                }
            }
        }

        if item_stack.quantity > 0 {
            // Fill empty pockets next
            for p in &mut self.pockets {
                match &p.contents {
                    Some(_) => continue,
                    None => {
                        let inserted_count = std::cmp::min(p.can_fit_quantity(volume, self.holds_cargo), item_stack.quantity);
                        p.set(item_id, inserted_count);
                        item_stack.quantity -= inserted_count;
                        if item_stack.quantity == 0 {
                            break;
                        }
                    }
                };
            }
        }
    }

    pub fn add_pocket_with_content(&mut self, item_stack: ItemStack) {
        let pocket = Pocket {
            volume: 6000,
            contents: Some(item_stack),
        };
        self.pockets.push(pocket);
    }

    pub fn add_pockets(&mut self, num_new_pockets: i32, pocket_volume: i32) {
        for _ in 0..num_new_pockets {
            self.pockets.push(Pocket {
                volume: pocket_volume,
                contents: None,
            });
        }
    }

    pub fn create_with_pockets(num_pockets: i32, pocket_volume: i32, holds_cargo: bool) -> Inventory {
        let mut pockets = Vec::new();
        for _ in 0..num_pockets {
            pockets.push(Pocket {
                volume: pocket_volume,
                contents: None,
            });
        }
        Inventory {
            pockets,
            building_id: 0,
            function_id: 0,
            holds_cargo,
        }
    }

    pub fn is_pocket_empty(&self, pocket_index: usize) -> bool {
        match self.get_pocket_contents(pocket_index) {
            Some(_) => false,
            None => true,
        }
    }

    pub fn get_pocket_contents(&self, pocket_index: usize) -> Option<ItemStack> {
        match self.pockets.get(pocket_index) {
            Some(p) => p.contents,
            None => None,
        }
    }

    pub fn has(&self, item_stacks: &Vec<ItemStack>) -> bool {
        let merged_stacks = ItemStack::merge_multiple(item_stacks);
        for stack in merged_stacks {
            let mut required = stack.quantity;
            for p in self.pockets.iter() {
                required -= match &p.contents {
                    Some(c) => {
                        if c.item_id == stack.item_id {
                            c.quantity
                        } else {
                            0
                        }
                    }
                    None => 0,
                };
            }

            if required > 0 {
                return false;
            }
        }

        true
    }

    pub fn fits(&self, item_stack: ItemStack) -> bool {
        self.fits_all(&vec![item_stack])
    }

    pub fn fits_all(&self, item_stacks: &Vec<ItemStack>) -> bool {
        let mut inventory_copy = self.clone();
        for &stack in item_stacks.iter() {
            if !inventory_copy.add(stack) {
                return false;
            }
        }
        true
    }

    pub fn fits_all_after_remove(&self, to_add: &Vec<ItemStack>, to_remove: &Vec<ItemStack>) -> bool {
        let mut inventory_copy = self.clone();
        inventory_copy.remove(to_remove);
        inventory_copy.fits_all(to_add)
    }

    pub fn is_empty(&self) -> bool {
        for p in self.pockets.iter() {
            match &p.contents {
                Some(c) => {
                    if c.quantity > 0 {
                        return false;
                    }
                }
                None => (),
            };
        }
        true
    }

    pub fn remove(&mut self, item_stacks: &Vec<ItemStack>) -> bool {
        let mut pockets_copy = self.pockets.clone();

        for &stack in item_stacks.iter() {
            if stack.quantity <= 0 {
                continue;
            }
            let item_id = stack.item_id;
            let mut quantity = stack.quantity;

            while quantity > 0 {
                let mut smallest_pocket: Option<&mut Pocket> = None;

                // Compare current pocket with the smallest we found
                for p in &mut pockets_copy {
                    if let Some(content) = p.contents.as_ref() {
                        if content.item_id == item_id {
                            if let Some(sp) = &smallest_pocket {
                                if sp.contents.as_ref().unwrap().quantity <= content.quantity {
                                    // Our current smallest pocket is smaller than this new pocket's quantity, skip
                                    continue;
                                }
                            }
                            // No current smallest pocket, or its quantity is larger than this new pocket's quantity.
                            smallest_pocket = Some(p);
                        }
                    }
                }

                // Reduce quantity from our smallest pocket found
                match smallest_pocket {
                    None => {
                        return false;
                    }
                    Some(p) => {
                        let mut pocket_quantity = p.contents.as_ref().unwrap().quantity;
                        if p.contents.as_ref().unwrap().quantity >= quantity {
                            pocket_quantity -= quantity;
                            quantity = 0;
                            if pocket_quantity == 0 {
                                p.contents = None;
                            } else {
                                if let Some(c) = p.contents.as_mut() {
                                    c.quantity = pocket_quantity;
                                }
                            }
                        } else {
                            quantity -= pocket_quantity;
                            p.contents = None;
                        }
                    }
                }
            }
        }
        self.pockets = pockets_copy;
        true
    }

    pub fn remove_at(&mut self, pocket_index: usize) -> Option<ItemStack> {
        let pocket = self.pockets.get(pocket_index);
        if let Some(p) = pocket {
            if let Some(stack) = p.contents.as_ref() {
                let return_stack = stack.clone();
                self.set_at(pocket_index, None);
                return Some(return_stack);
            }
        };
        None
    }

    pub fn remove_quantity_at(&mut self, pocket_index: usize, quantity: i32) -> Option<ItemStack> {
        let pocket = self.pockets.get(pocket_index);
        if let Some(p) = pocket {
            if let Some(stack) = p.contents.as_ref() {
                let new_stack = ItemStack {
                    quantity: stack.quantity - quantity,
                    item_id: stack.item_id,
                };
                if new_stack.quantity < 0 {
                    return None;
                }
                let return_stack = Some(ItemStack {
                    quantity,
                    item_id: stack.item_id,
                });
                self.set_at(pocket_index, if new_stack.quantity == 0 { None } else { Some(new_stack) });
                return return_stack;
            }
        };
        None
    }

    pub fn add_at(&mut self, pocket_index: usize, contents: ItemStack) -> bool {
        let quantity = contents.quantity;

        if quantity <= 0 {
            return true;
        }

        let item_id = contents.item_id;

        let volume = if self.holds_cargo {
            match StaticDataIndex::shared().cargos.by_id(item_id) {
                Some(c) => c.volume,
                None => return false, // invalid cargo id, probably 0.
            }
        } else {
            match StaticDataIndex::shared().items.by_id(item_id) {
                Some(i) => i.volume,
                None => return false, // invalid item id, probably 0.
            }
        };

        let pocket = &self.pockets[pocket_index];
        let inserted_quantity = std::cmp::min(pocket.can_fit_quantity(volume, self.holds_cargo), quantity);

        let mut item_stack = ItemStack {
            item_id: item_id,
            quantity: inserted_quantity,
        };

        if pocket.contents.is_some() {
            let current = pocket.contents.unwrap();
            if current.item_id != item_id {
                return false;
            }
            item_stack.quantity += current.quantity;
        }

        let mut inventory_copy = self.clone();

        inventory_copy.set_at(pocket_index, Some(item_stack));

        if quantity > inserted_quantity {
            let remaining_stack = ItemStack {
                item_id: item_id,
                quantity: quantity - inserted_quantity,
            };
            if !inventory_copy.add(remaining_stack) {
                return false;
            }
        }

        self.pockets = inventory_copy.pockets;

        return true;
    }

    pub fn set_at(&mut self, pocket_index: usize, contents: Option<ItemStack>) {
        let pocket = self.pockets.get(pocket_index);

        // set an empty pocket instead of a pocket with a quantity of 0
        let pocket_contents = match contents {
            Some(c) => {
                if c.quantity == 0 {
                    None
                } else {
                    contents
                }
            }
            None => None,
        };

        if let Some(p) = pocket {
            self.pockets[pocket_index] = Pocket {
                contents: pocket_contents,
                volume: p.volume,
            };
        }
    }

    pub fn remove_empty_pockets(&mut self, min_remaining: usize) {
        self.pockets.retain(|p| p.contents.is_some());
        if min_remaining > self.pockets.iter().count() {
            self.add_pockets((min_remaining - self.pockets.iter().count()) as i32, 6000);
        }
    }
}