{ lib, pkgs, ... }:
{
  projectRootFile = "flake.nix";

  programs = {
    actionlint.enable = true;
    nixfmt.enable = true;
    prettier.enable = true;
    rustfmt.enable = true;
  };

  programs.prettier.settings.editorconfig = true;

  settings.formatter = {
    editorconfig-checker = {
      command = lib.getExe pkgs.editorconfig-checker;
      includes = [ "*" ];
      priority = 1;
    };

    prettier.includes = lib.mkForce [
      "*.md"
      "*.markdown"
      "**/*.md"
      "**/*.markdown"
      "frontend/*.json"
      "frontend/*.mjs"
      "frontend/*.ts"
      "frontend/app/**/*.css"
      "frontend/app/**/*.ts"
      "frontend/app/**/*.tsx"
      "frontend/plugin-local/*.json"
      "frontend/plugin-local/*.ts"
      "frontend/plugin-local/src/**/*.css"
      "frontend/plugin-local/src/**/*.html"
      "frontend/plugin-local/src/**/*.ts"
    ];
  };
}
