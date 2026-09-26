variables {
  environment = "dev"
  vpc_cidr    = "10.0.0.0/16"
  zones       = ["a", "b"]
}

run "allocates_tiers_then_zones" {
  command = plan

  assert {
    condition = jsonencode({ for key, subnet in output.subnets : key => subnet.cidr }) == jsonencode({
      "public-a"  = "10.0.0.0/24"
      "public-b"  = "10.0.1.0/24"
      "private-a" = "10.0.16.0/20"
      "private-b" = "10.0.32.0/20"
      "data-a"    = "10.0.48.0/24"
      "data-b"    = "10.0.49.0/24"
    })
    error_message = "unexpected CIDRs"
  }
}

run "names_tags_and_public_subnets" {
  command = plan

  variables {
    environment = "prod"
    tags        = { Team = "net", Tier = "overridden" }
  }

  assert {
    condition     = output.subnets["private-b"].name == "net-prod-private-b"
    error_message = "unexpected name"
  }
  assert {
    condition     = jsonencode(output.subnets["data-a"].tags) == jsonencode({ Team = "net", Environment = "prod", Tier = "data" })
    error_message = "unexpected tags"
  }
  assert {
    condition     = jsonencode(output.public_subnets) == jsonencode(["net-prod-public-a", "net-prod-public-b"])
    error_message = "unexpected public subnets"
  }
  assert {
    condition     = output.subnets["public-a"].public && !output.subnets["data-b"].public
    error_message = "unexpected public flags"
  }
}

run "capacity_per_tier" {
  command = plan

  assert {
    condition     = jsonencode(output.capacity) == jsonencode({ public = 502, private = 8182, data = 502 })
    error_message = "unexpected capacity"
  }
}

run "rejects_an_unknown_environment" {
  command         = plan
  variables { environment = "qa" }
  expect_failures = [var.environment]
}

run "rejects_a_prefix_outside_16_to_20" {
  command         = plan
  variables { vpc_cidr = "10.0.0.0/24" }
  expect_failures = [var.vpc_cidr]
}

run "rejects_repeated_zones" {
  command         = plan
  variables { zones = ["a", "a"] }
  expect_failures = [var.zones]
}
