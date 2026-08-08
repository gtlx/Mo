#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Tauri 标准入口:bin 只调用 lib(desktop_pet_lib),
// 所有模块(pet_render 等)在 lib.rs 统一注册,避免双份编译。
fn main() {
    desktop_pet_lib::app::run();
}
