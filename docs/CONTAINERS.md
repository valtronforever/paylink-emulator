# Headless container

```sh
docker build -t paylink-emulator .
docker run --rm --name paylink-test \
  -e PAYLINK_CONTROL_TOKEN=isolated-container-test-token \
  paylink-emulator serve
```

Both listeners remain loopback-only **inside the container**. Publishing Docker ports will not expose them. Run the browser/test runner in the same network namespace, for example `--network container:paylink-test`, or use native CLI binaries directly on your CI host. Do not loosen the control bind to a public interface. On Linux only, an explicit `--network host` is another local CI option; the native process approach is simpler on macOS/Windows.

The image contains only the headless binary and needs no graphics drivers, real terminal, bank network or production database. The Dockerfile builds from the committed lockfile, and the process runs as UID 10001. Bind a writable evidence directory and set `--journal /evidence/journal.json` to retain results after removing the container. Pass a test-only token through the environment or your runner's secret mechanism.
