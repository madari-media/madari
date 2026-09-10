//! Generates the Swift bindings for the app. See scripts/build-ios-native.sh.
fn main() {
    uniffi::uniffi_bindgen_main()
}
