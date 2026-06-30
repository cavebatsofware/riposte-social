{
  description = "riposte-social dev + deploy tooling";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        # Userland tooling for the bun deploy CLI (`bun tooling/cli.ts ...`) and local dev.
        # Rust is intentionally NOT pinned here: this toolchain is managed by rustup, and a
        # nix-built rustc can stop exec-ing after a NixOS glibc bump. rustup is included so a
        # fresh machine can `rustup default stable`; the daemon-side docker client comes from
        # the host (system docker), with docker-compose bundled as a v2 fallback.
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            bun
            sops
            age
            jq
            openssl
            docker-compose
            rustup
            zsh
          ];

          shellHook = ''
            export SOPS_AGE_KEY_FILE="''${SOPS_AGE_KEY_FILE:-$HOME/.ssh/age.txt}"

            # nix develop drops you into a bare bash. For interactive sessions,
            # re-exec into zsh so your normal ~/.zshrc loads and the shell matches
            # the rest of your terminal. Guarded to interactive shells so
            # `nix develop --command ...` and CI keep running under bash.
            if [[ $- == *i* ]]; then
              export SHELL=${pkgs.zsh}/bin/zsh
              exec ${pkgs.zsh}/bin/zsh
            fi
          '';
        };
      }
    );
}
