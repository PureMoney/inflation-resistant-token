# Start from a Linux image with the exact old Rust version we need
FROM rust:1.68.2

# Set shell to bash
SHELL ["/bin/bash", "-c"]

# Install basic dependencies needed for Solana and building crates
RUN apt-get update && apt-get install -y curl build-essential libudev-dev pkg-config

# --- Solana CLI Installation ---
# Install the exact Solana version required: 1.17.0
RUN sh -c "$(curl -sSfL https://release.solana.com/v1.17.0/install)"
# Add Solana to the system's PATH so it can be found by other tools
ENV PATH="/root/.local/share/solana/install/active_release/bin:$PATH"

# --- Anchor Version Manager (AVM) and Anchor CLI Installation ---
# Install AVM, the tool to manage Anchor versions
RUN cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
# Use AVM to install the exact Anchor version required: 0.29.0
RUN avm install 0.29.0
# Set this version as the one to use
RUN avm use 0.29.0

# Set the working directory inside the container to /app
WORKDIR /app

# The default command when the container starts is to open a bash shell
CMD ["/bin/bash"]```