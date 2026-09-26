locals {
  plan = flatten([
    for tier in var.tiers : [
      for zone in var.zones : {
        key     = "${tier.name}-${zone}"
        tier    = tier.name
        zone    = zone
        newbits = tier.newbits
        public  = tier.public
      }
    ]
  ])
  cidrs = cidrsubnets(var.vpc_cidr, [for entry in local.plan : entry.newbits]...)
  subnets = {
    for index, entry in local.plan : entry.key => {
      cidr   = local.cidrs[index]
      zone   = entry.zone
      tier   = entry.tier
      public = entry.public
      name   = "net-${var.environment}-${entry.tier}-${entry.zone}"
      tags   = merge(var.tags, { Environment = var.environment, Tier = entry.tier })
    }
  }
}
