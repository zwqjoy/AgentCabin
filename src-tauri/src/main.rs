#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agentcabin_desktop_lib::run_core_server();
}
