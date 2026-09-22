# pi-computer-use native macOS bridge

The Swift sources in `macos/` are vendored from
[`injaneity/pi-computer-use`](https://github.com/injaneity/pi-computer-use),
version 0.5.1, commit `4b8dbd7eaa13328ab1a8a4b55d0be0b077de7d62`.

They are distributed under the included MIT license. AgentCabin compiles and
signs the bridge during its macOS build, starts it as a Host-owned persistent
daemon, and communicates over the upstream protocol v6 Unix socket contract.
