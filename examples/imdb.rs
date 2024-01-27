use std::fs::File;

use attohttpc::ResponseExt;

fn main() -> attohttpc::Result {
    env_logger::init();

    let resp = attohttpc::get("https://datasets.imdbws.com/title.basics.tsv.gz").send()?;
    println!("Status: {:?}", resp.status());
    println!("Headers:\n{:#?}", resp.headers());
    if resp.is_success() {
        let file = File::create("title.basics.tsv.gz")?;
        let n = resp.into_body().write_to(file)?;
        println!("Wrote {n} bytes to the file.");
    }

    Ok(())
}
