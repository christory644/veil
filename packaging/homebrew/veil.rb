class Veil < Formula
  desc "Cross-platform, GPU-accelerated terminal workspace manager for AI coding agents"
  homepage "https://github.com/veil-term/veil"
  license any_of: ["MIT", "Apache-2.0"]

  # Updated by release CI
  url "https://github.com/veil-term/veil/archive/refs/tags/v#{version}.tar.gz"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "veil", shell_output("#{bin}/veil --version")
  end
end
