# This cask intentionally follows the desktop release pipeline's stable R2 DMG
# URL: the object is replaced on each signed release, and the in-app updater owns
# versioned update checks after the first Homebrew install.
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
