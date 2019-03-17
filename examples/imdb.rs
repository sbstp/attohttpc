use std::fs::File;

use lynx::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let (status, headers, body) = lynx::get("https://datasets.imdbws.com/title.basics.tsv.gz").send()?;
    println!("Headers:\n{:?} {:#?}", status, headers);
    if status.is_success() {
        let file = File::create("title.basics.tsv.gz")?;
        let n = body.write_to(file)?;
        println!("Wrote {} bytes to the file.", n);
    }

    Ok(())
}
