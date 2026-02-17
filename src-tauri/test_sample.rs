#[test]
fn test_sample_path_resolution() {
    let code = r#"
sample_path = "C:/Development/Workspaces/MusicAgent/Samples/African Vocals Sung/"
live_loop :verse1_vocals do
  6.times do
    sample sample_path + "african-vocals-gubulah-high.wav", amp: 1.5
    sleep 5.6
  end
  stop
end
"#;
    let parsed = crate::audio::parser::parse_code(code).unwrap();
    eprintln!("Parsed: {:#?}", parsed);
    // Check that we get the right sample name
    fn find_samples(cmds: &[crate::audio::parser::ParsedCommand]) -> Vec<String> {
        let mut result = Vec::new();
        for cmd in cmds {
            match cmd {
                crate::audio::parser::ParsedCommand::PlaySample { name, .. } => {
                    result.push(name.clone());
                }
                crate::audio::parser::ParsedCommand::Loop { commands, .. }
                | crate::audio::parser::ParsedCommand::WithFx { commands, .. }
                | crate::audio::parser::ParsedCommand::TimesLoop { commands, .. } => {
                    result.extend(find_samples(commands));
                }
                _ => {}
            }
        }
        result
    }
    let samples = find_samples(&parsed);
    eprintln!("Found sample names: {:?}", samples);
    assert!(!samples.is_empty(), "Should have found sample names");
    assert!(samples[0].contains("african-vocals"), "Sample name should contain resolved path: {}", samples[0]);
}
