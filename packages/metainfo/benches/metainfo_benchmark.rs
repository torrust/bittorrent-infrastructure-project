#[cfg(feature = "bench")]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use metainfo::{DirectAccessor, Metainfo, MetainfoBuilder};

const MULTI_KB_METAINFO: &[u8; 30004] = include_bytes!("multi_kb.metainfo");

fn bench_build_multi_kb_metainfo(file_content: &[u8]) {
    let direct_accessor = DirectAccessor::new("100MBFile", file_content);

    MetainfoBuilder::new().build(2, direct_accessor, |_| ()).unwrap();
}

fn bench_parse_multi_kb_metainfo(metainfo: &[u8]) {
    Metainfo::from_bytes(metainfo).unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    let file_content = vec![55u8; 10 * 1024 * 1024];
    let file_content_buffer = file_content.as_slice();

    c.bench_function("metainfo build multi kb", |b| {
        b.iter(|| bench_build_multi_kb_metainfo(black_box(file_content_buffer)))
    });

    c.bench_function("metainfo parse multi kb", |b| {
        b.iter(|| bench_parse_multi_kb_metainfo(black_box(MULTI_KB_METAINFO)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
