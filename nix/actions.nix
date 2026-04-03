{ inputs, ... }:
let
  inherit (inputs.nix-actions.lib) steps;
  inherit (inputs.nix-actions.lib) platforms;
  inherit (inputs.nix-actions.lib) mkCachixSteps;
in
{
  imports = [ inputs.actions-nix.flakeModules.default ];
  flake.actions-nix = {
    pre-commit.enable = true;
    defaultValues = {
      jobs = {
        runs-on = "ubuntu-latest";
      };
    };
    workflows = {
      ".github/workflows/mr.yaml" = inputs.nix-actions.lib.mkConform { };
      ".github/workflows/linting.yaml" = inputs.nix-actions.lib.mkClippy { };
      ".github/workflows/build.yaml" = {
        on = {
          push = { };
          pull_request = { };
        };
        env = {
          CARGO_TERM_COLOR = "always";
        };
        jobs = {
          nix-build = {
            strategy.matrix.platform = [
              platforms.linux
              platforms.linux_aarch64
            ];
            runs-on = "\${{ matrix.platform.runs-on }}";
            steps = [
              steps.checkout
              steps.installNix
              {
                name = "Build";
                run = "nix build .";
              }
            ]
            ++ mkCachixSteps { };
          };
        };
      };
    };
  };
}
