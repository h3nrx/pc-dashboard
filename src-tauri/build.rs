fn main() {
    // FairPlay decryption for AirPlay mirroring (playfair, GPL-3.0)
    cc::Build::new()
        .files(["playfair/hand_garble.c", "playfair/modified_md5.c", "playfair/omg_hax.c", "playfair/playfair.c", "playfair/sap_hash.c"])
        .warnings(false)
        .compile("playfair");
    println!("cargo:rerun-if-changed=playfair");
    tauri_build::build()
}
