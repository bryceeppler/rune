use super::Packets;

fn typed(units: impl IntoIterator<Item = u16>) -> Vec<String> {
    let mut packets = Packets::default();
    units.into_iter().map(|unit| packets.unit(unit)).collect()
}

#[test]
fn each_unit_of_the_basic_plane_types_itself() {
    assert_eq!(typed("aé€".encode_utf16()), ["a", "é", "€"]);
}

#[test]
fn a_surrogate_pair_types_its_character_once_both_halves_arrive() {
    assert_eq!(typed("😀x🎉".encode_utf16()), ["", "😀", "x", "", "🎉"]);
}

#[test]
fn a_half_without_its_partner_types_a_replacement_character() {
    let (high, low) = (0xd83d, 0xde00);
    assert_eq!(typed([high, u16::from(b'a')]), ["", "\u{fffd}a"]);
    assert_eq!(typed([low, high, high, low]), ["\u{fffd}", "", "\u{fffd}", "😀"]);
}
