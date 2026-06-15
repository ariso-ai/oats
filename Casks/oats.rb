# release-publish.sh uploads each signed release to this stable R2 DMG key.
# Homebrew uses that first-install artifact; Tauri's in-app updater owns
# versioned updates after install via desktop/latest.json.
cask "oats" do
  version :latest
  sha256 :no_check

  url "https://pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/oats.dmg",
      verified: "pub-dd2807d512d34e55b8a863f675ea8e6e.r2.dev/desktop/"
  name "Oats"
  desc "AI meeting recorder and notes app"
  homepage "https://ariso.ai/"

  depends_on macos: :sonoma

  app "oats.app"
end
