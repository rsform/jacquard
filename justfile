default:
    @just --list

# Run pre-commit hooks on all files, including autoformatting
pre-commit-all:
    pre-commit run --all-files

# Run 'cargo run' on the project
run *ARGS:
    cargo run {{ARGS}}

# Run 'bacon' to run the project (auto-recompiles)
watch *ARGS:
	bacon --job run -- -- {{ ARGS }}

# Run the OAuth timeline example
example-oauth *ARGS:
    cargo run -p jacquard --example oauth_timeline --features fancy -- {{ARGS}}

# Create a simple post
example-create-post *ARGS:
    cargo run -p jacquard --example create_post --features fancy -- {{ARGS}}

# Create a post with an image
example-post-image *ARGS:
    cargo run -p jacquard --example post_with_image --features fancy -- {{ARGS}}

# Update profile display name and description
example-update-profile *ARGS:
    cargo run -p jacquard --example update_profile --features fancy -- {{ARGS}}

# Fetch public AT Protocol feed (no auth)
example-public-feed:
    cargo run -p jacquard --example public_atproto_feed

# Create a WhiteWind blog post
example-whitewind-create *ARGS:
    cargo run -p jacquard --example create_whitewind_post --features fancy -- {{ARGS}}

# Read a WhiteWind blog post
example-whitewind-read *ARGS:
    cargo run -p jacquard --example read_whitewind_posts --features fancy,api_full -- {{ARGS}}

# Read info about a Tangled git repository
example-tangled-repo *ARGS:
    cargo run -p jacquard --example read_tangled_repo --features fancy,api_full -- {{ARGS}}

# Resolve a handle to its DID document
example-resolve-did *ARGS:
    cargo run -p jacquard --example resolve_did -- {{ARGS}}

# Update Bluesky preferences
example-update-preferences *ARGS:
    cargo run -p jacquard --example update_preferences --features fancy -- {{ARGS}}

# Run the Axum server example
example-axum:
    cargo run -p jacquard-axum --example axum_server --features jacquard/fancy
