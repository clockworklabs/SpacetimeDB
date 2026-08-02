use std::process::Command;

// Note: This isn't perfectly correct, and may look a little wrong if the args have spaces, quotes,
// or other characters that need escaping, but it's a decent representation.
pub fn print_command(cmd: &Command) {
    // program() is Option<OsStr>, args() is an iterator of OsStr
    let program = cmd.get_program().to_string_lossy();
    let args = cmd
        .get_args()
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    let dir = cmd.get_current_dir().map(|d| d.to_string_lossy().into_owned());

    match dir {
        Some(d) => println!("$> {} {}   (cwd = {})", program, args, d),
        None => println!("$> {} {}", program, args),
    }
}
