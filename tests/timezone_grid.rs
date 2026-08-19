use omarchy_world_clock::timezone_grid::timezone_at;

#[test]
fn bundled_map_lookup_covers_representative_world_regions() {
    let cases = [
        (48.11109, -1.67431, "Europe/Paris"),
        (35.6762, 139.6503, "Asia/Tokyo"),
        (28.6139, 77.2090, "Asia/Kolkata"),
        (40.7128, -74.0060, "America/New_York"),
        (49.2827, -123.1207, "America/Vancouver"),
        (19.4326, -99.1332, "America/Mexico_City"),
        (-33.8688, 151.2093, "Australia/Sydney"),
        (-33.9249, 18.4241, "Africa/Johannesburg"),
    ];

    for (latitude, longitude, expected) in cases {
        assert_eq!(
            timezone_at(latitude, longitude).as_deref(),
            Some(expected),
            "unexpected timezone at {latitude}, {longitude}"
        );
    }
}

#[test]
fn bundled_map_lookup_rejects_oceans_and_invalid_coordinates() {
    assert_eq!(timezone_at(0.0, -140.0), None);
    assert_eq!(timezone_at(f64::NAN, 0.0), None);
    assert_eq!(timezone_at(91.0, 0.0), None);
    assert_eq!(timezone_at(0.0, 181.0), None);
}
