#[macro_use]
extern crate bencode;

#[test]
fn positive_ben_map_macro() {
    let result = (ben_map! {
        "key" => ben_bytes!("value")
    })
    .encode();

    /* cspell:disable-next-line */
    assert_eq!("d3:key5:valuee".as_bytes(), &result[..]);
}

#[test]
fn positive_ben_list_macro() {
    let result = (ben_list!(ben_int!(5))).encode();

    /* cspell:disable-next-line */
    assert_eq!("li5ee".as_bytes(), &result[..]);
}
