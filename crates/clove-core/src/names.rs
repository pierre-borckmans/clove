//! Friendly names for clones created without a branch (`clove new`).

const ADJ: &[&str] = &[
    "amber", "brisk", "calm", "deft", "eager", "fresh", "glad", "hardy", "keen", "lucid", "mellow", "nimble",
    "quiet", "rapid", "sly", "snug", "spry", "sunny", "swift", "tidy", "vivid", "wise", "bold", "crisp",
    "dapper", "fond", "jolly", "lively", "plucky", "zesty",
];
const NOUN: &[&str] = &[
    "basil",
    "bay",
    "cumin",
    "dill",
    "fennel",
    "ginger",
    "hyssop",
    "lovage",
    "mace",
    "mint",
    "nutmeg",
    "oregano",
    "pepper",
    "sage",
    "savory",
    "sorrel",
    "tarragon",
    "thyme",
    "anise",
    "caraway",
    "chive",
    "cress",
    "galangal",
    "juniper",
    "lemongrass",
    "marjoram",
    "mustard",
    "paprika",
    "saffron",
    "sumac",
];

fn seed() -> u64 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    // splitmix64 so consecutive calls differ
    let mut z = t ^ (std::process::id() as u64).rotate_left(32);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// `adjective-spice`, retried until `free` accepts it.
pub fn generate(free: impl Fn(&str) -> bool) -> String {
    let mut s = seed();
    for i in 0..64 {
        let name = format!(
            "{}-{}",
            ADJ[(s % ADJ.len() as u64) as usize],
            NOUN[((s >> 16) % NOUN.len() as u64) as usize]
        );
        let name = if i < 32 { name } else { format!("{name}-{i}") };
        if free(&name) {
            return name;
        }
        s = s.rotate_left(7).wrapping_add(0x9e3779b97f4a7c15);
    }
    format!("clove-{}", s % 100_000)
}

#[cfg(test)]
mod tests {
    #[test]
    fn avoids_taken() {
        let first = super::generate(|_| true);
        let second = super::generate(|n| n != first);
        assert_ne!(first, second);
    }
}
