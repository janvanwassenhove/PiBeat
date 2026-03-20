use sonic_daw_lib::audio::parser;
use sonic_daw_lib::audio::engine::AudioCommand;

#[test]
fn disco_groove_parse_test() {
    let code = include_str!("../../fidelity/fixtures/disco_groove.rb");
    let parsed = parser::parse_code(code).expect("parse should succeed");
    println!("Total parsed commands: {}", parsed.len());
    for (i, cmd) in parsed.iter().enumerate() {
        println!("  [{}] {:?}", i, cmd);
    }
    let audio = parser::commands_to_audio(&parsed, 122.0);
    println!("\nTotal audio events: {}", audio.len());
    let notes = audio.iter().filter(|(_, cmd)| matches!(cmd, AudioCommand::PlayNote { .. })).count();
    let samples = audio.iter().filter(|(_, cmd)| matches!(cmd, AudioCommand::PlaySample { .. })).count();
    println!("Notes: {}, Samples: {}", notes, samples);
    assert!(parsed.len() > 0, "Should produce parsed commands");
    assert!(audio.len() > 0, "Should produce audio events");
}
