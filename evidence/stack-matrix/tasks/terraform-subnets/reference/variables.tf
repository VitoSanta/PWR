variable "environment" {
  type = string
  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "environment must be dev, staging or prod."
  }
}

variable "vpc_cidr" {
  type = string
  validation {
    condition     = can(cidrhost(var.vpc_cidr, 0)) && can(regex("^[0-9.]+/[0-9]+$", var.vpc_cidr)) && try(tonumber(split("/", var.vpc_cidr)[1]) >= 16 && tonumber(split("/", var.vpc_cidr)[1]) <= 20, false)
    error_message = "vpc_cidr must be an IPv4 CIDR from /16 to /20."
  }
}

variable "zones" {
  type = list(string)
  validation {
    condition     = length(var.zones) >= 1 && length(var.zones) <= 4 && length(distinct(var.zones)) == length(var.zones)
    error_message = "zones must be 1 to 4 distinct names."
  }
}

variable "tiers" {
  type = list(object({
    name    = string
    newbits = number
    public  = bool
  }))
  default = [
    { name = "public", newbits = 8, public = true },
    { name = "private", newbits = 4, public = false },
    { name = "data", newbits = 8, public = false },
  ]
}

variable "tags" {
  type    = map(string)
  default = {}
}
