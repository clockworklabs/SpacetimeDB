#!/bin/bashtable

if [ "$DESCRIBE_TEST" = 1 ] ; then
        echo "This tests the iter_filtered method in reducers."
        exit
fi

set -euox pipefail

source "./test/lib.include"

cat >> "${PROJECT_PATH}/Cargo.toml" << EOF
rand = { version = "0.8.5", default-features = false, features = ["small_rng"]}

EOF

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{println, spacetimedb, query};
use rand::{Rng, SeedableRng};

#[derive(PartialEq)]
#[spacetimedb(table)]
pub struct Person {
    #[unique]
    id: i32,

    name: String,
    age: u64,
    employee_id: String,
}

// wanna generate a lot of nonsense data to ensure we go through multiple buffers
#[spacetimedb(reducer)]
pub fn insert_random_people(count: i32) {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(0xdeadbeef);
    let first_names = [
        "Anthony",
        "Tony",
        "Antonio",
        "Barbara",
        "Charles",
        "Daniel",
        "Danyl",
        "Darkholder Fleshwright",
        "Dan",
        "David",
        "Droog",
        "Elizabeth",
        "Liz",
        "James",
        "Jim",
        "Jimmy",
        "Jennifer",
        "Jen",
        "John",
        "Linda",
        "Lindy",
        "Margaret",
        "Marge",
        "Mary",
        "Michael",
        "Richard",
        "Dick",
        "Robert",
        "Thomas",
        "Tom",
    ];

    let last_names = [
        "Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis", "Wilson",
        "Moore", "Taylor", "Anderson", "Thomas", "Jackson", "Thompson", "White", "Harris",
        "Martin", "Green", "Walker", "Hall", "Wood", "Lewis", "Hill", "Walker", "Sanchez",
        "Carter", "Robinson", "Cook", "Morgan",
    ];

    for id in 0..count {
        let first_name = first_names[rng.gen_range(0..first_names.len())];
        let last_name = last_names[rng.gen_range(0..last_names.len())];
        let age = rng.gen_range(0u64..101);
        let dept = rng.gen_range(0u64..200);
        let in_dept = rng.gen_range(0u64..200);
        Person::insert(Person {
            id,
            name: format!("{last_name}, {first_name}"),
            age,
            employee_id: format!("{dept} {in_dept}"),
        }).unwrap();
    }
}

// wanna generate a lot of these to ensure we go through multiple buffers
#[spacetimedb(reducer)]
pub fn most_common_ages() {
    println!(
        "Looking for ages"
    );
    let mut hist = [0u32; 101];
    let mut seen = 0u32;

    for person in Person::iter() {
        hist[person.age as usize] += 1;
        seen += 1;
    }

    let (age, count) = hist.iter()
        .enumerate()
        .max_by_key(|(_, count)| **count)
        .unwrap();

    println!(
        "Most common age: {age}, count: {count}"
    );
    // we'll just test if this actually went through everybody we inserted
    println!("TOTAL SEEN: {seen}");
}

#[spacetimedb(reducer)]
pub fn filtering_works_properly() {
    // these will both run BSatnCompactor through the motions
    let mut retirees_via_filtered: Vec<Person> = query!(|person: Person| person.age >= 65).collect();
    let mut retirees_via_bad: Vec<Person> = Person::iter().filter(|person: &Person| person.age >= 65).collect();
    retirees_via_filtered.sort_by_key(|person| person.id);
    retirees_via_bad.sort_by_key(|person| person.id);

    if retirees_via_bad == retirees_via_filtered {
        println!("PATHS EQUIVALENT: YES");
    } else {
        println!("PATHS EQUIVALENT: NO");
    }
}

EOF

run_test cargo run publish -s -d --project-path "$PROJECT_PATH" --clear-database
[ "1" == "$(grep -c "reated new database" "$TEST_OUT")" ]
IDENT="$(grep "reated new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

# this should be enough to go through a couple of bsatn buffers
run_test cargo run call "$IDENT" insert_random_people '[5000]'

run_test cargo run call "$IDENT" most_common_ages '[]'
run_test cargo run logs "$IDENT" 100
[ ' TOTAL SEEN: 5000' == "$(grep 'TOTAL SEEN:' "$TEST_OUT" | tail -n 4 | cut -d: -f4-)" ]

run_test cargo run call "$IDENT" filtering_works_properly '[]'
run_test cargo run logs "$IDENT" 100
[ ' PATHS EQUIVALENT: YES' == "$(grep 'PATHS EQUIVALENT:' "$TEST_OUT" | tail -n 4 | cut -d: -f4-)" ]
