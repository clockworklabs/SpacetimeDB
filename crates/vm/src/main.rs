use spacetimedb_vm::eval::{fibo, run_ast};
use spacetimedb_vm::program::Program;

use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_vm::expr::Code;
use std::env;

fn fib_vm(input: u64) -> Code {
    let p = &mut Program::new(AuthCtx::for_testing());
    let check = fibo(input);
    run_ast(p, check)
}

fn fib(n: u64) -> u64 {
    if n < 2 {
        return n;
    }

    fib(n - 1) + fib(n - 2)
}

fn main() {
    #![allow(clippy::disallowed_macros)]

    let mut args = env::args().skip(1);
    let first = args.next();

    let input = 25;
    match first.as_deref() {
        Some("native") => {
            println!("Run native fib..");
            println!("{:?}", fib(input))
        }
        _ => {
            println!("Run vm fib");
            println!("{:?}", fib_vm(input))
        }
    }
}
