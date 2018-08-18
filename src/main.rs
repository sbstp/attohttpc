#![feature(nll)]
#![feature(rust_2018_preview, uniform_paths)]

#[macro_use]
extern crate failure;
extern crate http;
extern crate url;

mod error;
mod parse;
mod request;

pub use request::Request;

fn main() {
    let mut r = Request::new("http://sbstp.ca");
    r.param("foo", 3);
    r.param("gee", true);
    let (status, headers, reader) = r.send().unwrap();
    println!("{:?} {:#?}", status, headers);
    println!("{}", reader.string().unwrap().len());
}
