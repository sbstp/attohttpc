use std::env;

use lynx::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let url: String = env::args().collect::<Vec<_>>().into_iter().nth(1).expect("missing url");

    let (status, headers, reader) = lynx::get(&url).send()?;
    println!("Status: {:?}", status);
    println!("Headers:\n{:#?}", headers);
    println!("Body:\n{}", reader.text()?);

    Ok(())
}
