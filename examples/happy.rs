use attohttpc::happy::connect;

fn main() {
    println!("{:#?}", connect("facebook.com:443"));
}
