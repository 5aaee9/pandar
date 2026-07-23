#[unsafe(no_mangle)]
pub extern "C" fn pandar_bambu_source_sentinel() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    #[test]
    fn sentinel_identifies_the_non_media_companion() {
        assert_eq!(super::pandar_bambu_source_sentinel(), 1);
    }
}
