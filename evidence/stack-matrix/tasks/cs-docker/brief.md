Containerize this API:

1. A multi-stage `Dockerfile` at the root of the repository: build with the .NET 10 SDK image, run on the ASP.NET Core 10 runtime image, listen on port 8080, run as a non-root user.
2. A `.dockerignore` beside it that keeps bin/, obj/ and tests out of the build context.
3. `compose.yaml` with the api service on port 8080 and a healthcheck on /health.
4. `scripts/smoke.sh`: builds the image, starts a container, waits until /health answers, creates an item with POST /items, checks that GET /stats reports it, then removes the container. It must exit non-zero on any failure.

Run the smoke script and make it pass. Reply with what you built and the smoke result.
