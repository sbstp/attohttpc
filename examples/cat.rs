use std::env;

use lynx::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let url: String = env::args().collect::<Vec<_>>().into_iter().nth(1).expect("missing url");

    let (status, headers, reader) = lynx::get(&url).send()?;
    println!("Headers:\n{:?} {:#?}", status, headers);
    println!();
    println!("Body:\n{}", reader.string()?);

    Ok(())
}
