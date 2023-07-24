#[macro_use]
extern crate bip_bencode;

#[test]
fn positive_ben_map_macro() {
    let result = (ben_map!{
        "key" => ben_bytes!("value")
    }).encode();

    assert_eq!("d3:key5:valuee".as_bytes(), &result[..]);
}

#[test]
fn positive_ben_list_macro() {
    let result = (ben_list!(
        ben_int!(5)
    )).encode();

    assert_eq!("li5ee".as_bytes(), &result[..]);
}