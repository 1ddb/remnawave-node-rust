variable "TAG" {
  default = "dev"
}

variable "REGISTRIES" {
  default = ["ghcr.io/1ddb/remnawave-node-rust"]
}

target "node" {
  name = variant

  matrix = {
    variant = ["plain"]
  }

  context    = "."
  dockerfile = "docker/Dockerfile"
  platforms  = ["linux/amd64", "linux/arm64"]

  tags = [
    for registry in REGISTRIES : "${registry}:${TAG}"
  ]
}

group "default" {
  targets = ["plain"]
}
