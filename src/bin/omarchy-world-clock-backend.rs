use anyhow::Result;
use omarchy_world_clock::backend;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Some(output) = backend::execute(&args)? {
        println!("{output}");
    }
    Ok(())
}
