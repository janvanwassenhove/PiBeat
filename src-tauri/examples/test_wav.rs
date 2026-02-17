fn main() {
    let files = [
        "C:/Development/Workspaces/MusicAgent/Samples/African Vocals Sung/african-vocals-gubulah-high.wav",
        "C:/Development/Workspaces/MusicAgent/Samples/African Vocals Sung/chorus-hetum-yoyo.wav",
        "C:/Development/Workspaces/MusicAgent/Samples/African Vocals Sung/african-vocals-weeh-oh-mid.wav",
        "C:/Development/Workspaces/MusicAgent/Samples/African Vocals Sung/zap-mama-style-3.wav",
    ];

    for path in &files {
        print!("Testing '{}': ", path);
        let p = std::path::Path::new(path);
        if !p.exists() {
            println!("FILE NOT FOUND");
            continue;
        }
        match hound::WavReader::open(path) {
            Ok(reader) => {
                let spec = reader.spec();
                let len = reader.len();
                println!("OK - {}ch, {}Hz, {} bits, {} samples",
                    spec.channels, spec.sample_rate, spec.bits_per_sample, len);
            }
            Err(e) => {
                println!("HOUND ERROR: {}", e);
            }
        }
    }
}
