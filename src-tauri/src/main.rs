// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// I might be retarded but I can't get this to work without it lmao.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    client_lib::run()
}
