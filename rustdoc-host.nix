{ config, pkgs, lib, ... }:

{
  # Basic system config
  networking.firewall.allowedTCPPorts = [ 80 443 ];

  # Rust toolchain for building docs
  environment.systemPackages = with pkgs; [
    rustup
    git
    cargo
  ];

  # Build script to generate docs
  environment.etc."rustdoc-build.sh" = {
    text = ''
      #!/usr/bin/env bash
      set -euo pipefail

      REPO_URL="''${1:-https://github.com/orual/jacquard.git}"
      BRANCH="''${2:-main}"
      BUILD_DIR="/var/www/rustdoc/build"
      OUTPUT_DIR="/var/www/rustdoc/docs"

      echo "Building docs from $REPO_URL ($BRANCH)..."

      # Clean and clone
      rm -rf "$BUILD_DIR"
      git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$BUILD_DIR"
      cd "$BUILD_DIR"

      # Build docs with all features for jacquard-api
      export RUSTDOCFLAGS="--html-in-header /etc/rustdoc-analytics.html"
      cargo doc \
        --no-deps \
        --workspace \
        --all-features \
        --document-private-items

      # Copy to serving directory
      rm -rf "$OUTPUT_DIR"
      cp -r target/doc "$OUTPUT_DIR"

      # Create index redirect
      cat > "$OUTPUT_DIR/index.html" <<EOF
      <!DOCTYPE html>
      <html>
      <head>
        <meta http-equiv="refresh" content="0; url=jacquard/index.html">
        <title>Jacquard Documentation</title>
      </head>
      <body>
        <p>Redirecting to <a href="jacquard/index.html">jacquard documentation</a>...</p>
      </body>
      </html>
      EOF

      chown -R nginx:nginx "$OUTPUT_DIR"
      echo "Build complete! Docs available at $OUTPUT_DIR"
    '';
    mode = "0755";
  };

  # Optional analytics snippet (empty by default)
  environment.etc."rustdoc-analytics.html" = {
    text = ''
      <!-- Add analytics/plausible/umami script here if desired -->
    '';
  };

  # Nginx to serve the docs
  services.nginx = {
    enable = true;
    recommendedGzipSettings = true;
    recommendedOptimisation = true;
    recommendedProxySettings = true;
    recommendedTlsSettings = true;

    virtualHosts."docs.example.com" = {
      # Set this to your actual domain
      # serverName = "docs.jacquard.dev";

      # For cloudflare tunnel, you don't need ACME here
      # If you want direct HTTPS:
      # enableACME = true;
      # forceSSL = true;

      root = "/var/www/rustdoc/docs";

      locations."/" = {
        tryFiles = "$uri $uri/ =404";
        extraConfig = ''
          # Cache static assets
          location ~* \.(css|js|woff|woff2)$ {
            expires 1y;
            add_header Cache-Control "public, immutable";
          }

          # CORS headers for cross-origin font loading
          location ~* \.(woff|woff2)$ {
            add_header Access-Control-Allow-Origin "*";
          }
        '';
      };
    };
  };

  # Create serving directory
  systemd.tmpfiles.rules = [
    "d /var/www/rustdoc 0755 nginx nginx -"
    "d /var/www/rustdoc/build 0755 nginx nginx -"
    "d /var/www/rustdoc/docs 0755 nginx nginx -"
  ];

  # Optional: systemd service for periodic rebuilds
  systemd.services.rustdoc-build = {
    description = "Build Jacquard documentation";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.bash}/bin/bash /etc/rustdoc-build.sh";
      User = "nginx";
    };
  };

  # Optional: timer to rebuild daily
  systemd.timers.rustdoc-build = {
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnCalendar = "daily";
      Persistent = true;
    };
  };

  # Optional: webhook receiver for rebuild-on-push
  # Uncomment if you want webhook triggers
  # services.webhook = {
  #   enable = true;
  #   hooks = {
  #     rebuild-docs = {
  #       execute-command = "/etc/rustdoc-build.sh";
  #       command-working-directory = "/tmp";
  #     };
  #   };
  # };
}
