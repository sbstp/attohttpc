use std::fs::File;

use attohttpc::HttpResult;

fn main() -> HttpResult {
    env_logger::init();

    let (status, headers, reader) = attohttpc::get("https://datasets.imdbws.com/title.basics.tsv.gz").send()?;
    println!("Status: {:?}", status);
    println!("Headers:\n{:#?}", headers);
    if status.is_success() {
        let file = File::create("title.basics.tsv.gz")?;
        let n = reader.write_to(file)?;
        println!("Wrote {} bytes to the file.", n);
    }

    Ok(())
}
