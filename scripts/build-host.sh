# Sourced by the run scripts (not run on its own): build the host binary and set $host.
# HOST_PROFILE=release (set by `run-all.sh --prod`) builds the optimized binary; default is the debug build.
case "${HOST_PROFILE:-debug}" in
  release)
    cargo build --quiet --release -p host
    host=target/release/host
    ;;
  *)
    cargo build --quiet -p host
    host=target/debug/host
    ;;
esac
