variable "REGISTRY" {
  default = "1ddb/remnawave-node-rust"
}

variable "TAG" {
  default = "latest"
}

group "default" {
  targets = ["rw-node"]
}

target "rw-node" {
  context    = "."
  dockerfile = "docker/Dockerfile"
  platforms  = ["linux/amd64", "linux/arm64"]
  tags       = ["${REGISTRY}:${TAG}"]
}
