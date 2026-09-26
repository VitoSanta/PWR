output "subnets" {
  value = local.subnets
}

output "public_subnets" {
  value = sort([for subnet in values(local.subnets) : subnet.name if subnet.public])
}

output "capacity" {
  value = {
    for tier in var.tiers : tier.name => sum([
      for subnet in values(local.subnets) : pow(2, 32 - tonumber(split("/", subnet.cidr)[1])) - 5 if subnet.tier == tier.name
    ])
  }
}
