defmodule Stock.MixProject do
  use Mix.Project

  def project do
    [app: :stock, version: "0.1.0", elixir: "~> 1.15", start_permanent: false, deps: []]
  end

  def application, do: [extra_applications: [:logger]]
end
