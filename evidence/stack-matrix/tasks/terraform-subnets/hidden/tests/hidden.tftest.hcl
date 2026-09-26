variables {
  environment = "staging"
  vpc_cidr    = "172.16.0.0/20"
  zones       = ["x", "y", "z"]
  tiers = [
    { name = "web", newbits = 4, public = true },
    { name = "db", newbits = 6, public = false },
  ]
}

run "custom_tiers_pack_in_order" {
  command = plan

  assert {
    condition = jsonencode({ for key, subnet in output.subnets : key => subnet.cidr }) == jsonencode({
      "web-x" = "172.16.0.0/24"
      "web-y" = "172.16.1.0/24"
      "web-z" = "172.16.2.0/24"
      "db-x"  = "172.16.3.0/26"
      "db-y"  = "172.16.3.64/26"
      "db-z"  = "172.16.3.128/26"
    })
    error_message = "unexpected CIDRs"
  }
  assert {
    condition     = jsonencode(output.capacity) == jsonencode({ web = 753, db = 177 })
    error_message = "unexpected capacity"
  }
  assert {
    condition     = jsonencode(output.public_subnets) == jsonencode(["net-staging-web-x", "net-staging-web-y", "net-staging-web-z"])
    error_message = "unexpected public subnets"
  }
  assert {
    condition     = output.subnets["db-z"].zone == "z" && output.subnets["db-z"].tier == "db"
    error_message = "unexpected zone or tier"
  }
}

run "rejects_a_prefix_too_wide" {
  command         = plan
  variables { vpc_cidr = "10.0.0.0/15" }
  expect_failures = [var.vpc_cidr]
}

run "rejects_something_not_a_cidr" {
  command         = plan
  variables { vpc_cidr = "not-a-cidr" }
  expect_failures = [var.vpc_cidr]
}

run "rejects_no_zones" {
  command         = plan
  variables { zones = [] }
  expect_failures = [var.zones]
}

run "rejects_five_zones" {
  command         = plan
  variables { zones = ["a", "b", "c", "d", "e"] }
  expect_failures = [var.zones]
}

run "accepts_the_edges" {
  command = plan
  variables {
    vpc_cidr = "10.8.0.0/20"
    zones    = ["one"]
    tiers    = [{ name = "only", newbits = 4, public = false }]
  }
  assert {
    condition     = jsonencode(output.public_subnets) == jsonencode([])
    error_message = "a tier-less public list should be empty"
  }
}
