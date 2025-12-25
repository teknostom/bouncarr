# Runtime-only image using pre-built binary
FROM debian:bookworm-slim

# Install CA certificates for HTTPS connections
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 bouncarr

# Set working directory
WORKDIR /app

# Copy the pre-built binary from target/release
COPY target/release/bouncarr /app/bouncarr

# Copy example config (user should mount their own config.yaml)
COPY config.example.yaml /app/config.example.yaml

# Change ownership
RUN chown -R bouncarr:bouncarr /app

# Switch to non-root user
USER bouncarr

# Expose the default port
EXPOSE 3000

# Run the application
CMD ["/app/bouncarr"]
