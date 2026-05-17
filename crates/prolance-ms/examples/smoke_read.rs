use prolance_ms::mzml::read_mzml;

fn main() {
    let path = std::env::args().nth(1).expect("need mzml path");
    let data = read_mzml(&path).unwrap();
    println!("run_id={}", data.run.run_id);
    println!("instrument={:?}", data.run.instrument);
    println!("start_time={:?}", data.run.start_time);
    println!("spectra={}", data.spectra.len());
    println!("ms1={:?} ms2={:?}", data.run.ms1_count, data.run.ms2_count);
    println!("chromatograms={}", data.chromatograms.len());
    if let Some(s) = data.spectra.first() {
        println!(
            "first: id={:?} ms_level={} rt={:?} peaks={} prec={:?}",
            s.native_id,
            s.ms_level,
            s.rt,
            s.mz.len(),
            s.precursor.as_ref().and_then(|p| p.mz),
        );
    }
    if let Some(c) = data.chromatograms.first() {
        println!(
            "chrom[0]: id={} type={:?} pts={}",
            c.chrom_id,
            c.chrom_type,
            c.time.len()
        );
    }
}
