FROM ubuntu:24.04

# Copy over SpacetimeDB
COPY --chmod=755 ./spacetimedb-standalone ./spacetimedb-cli /opt/spacetime/
RUN ln -s /opt/spacetime/spacetimedb-cli /usr/local/bin/spacetime

RUN mkdir /stdb && chmod 777 /stdb

# Create and switch to a non-root user
RUN useradd -m spacetime
USER spacetime

# Set working directory
WORKDIR /app

# Expose the necessary port
EXPOSE 3000
