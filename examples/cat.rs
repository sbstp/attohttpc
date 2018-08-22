use std::env;

use lynx::Request;

fn main() {
    env_logger::init();

    let url: String = env::args()
        .collect::<Vec<_>>()
        .into_iter()
        .nth(1)
        .expect("missing url");

    let r = Request::new(&url);

    let (status, headers, reader) = r.send().unwrap();
    println!("{:?} {:#?}", status, headers);
    println!("{}", reader.string().unwrap());
}
