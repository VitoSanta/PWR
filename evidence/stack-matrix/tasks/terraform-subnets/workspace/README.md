# subnets

A Terraform module (Terraform 1.9+, no providers) that plans a VPC's
subnets: which CIDR each tier gets in each zone, what it is called and
tagged, and how many addresses each tier has. Write it at the root:
`variables.tf`, `main.tf`, `outputs.tf`.

## Inputs

| Variable | Type | Rule (a violation fails validation on that variable) |
|---|---|---|
| `environment` | string | one of `dev`, `staging`, `prod` |
| `vpc_cidr` | string | an IPv4 CIDR whose prefix is /16 to /20 |
| `zones` | list(string) | 1 to 4 names, all different |
| `tiers` | list(object({ name = string, newbits = number, public = bool })) | default: `public` (8, public), `private` (4), `data` (8) |
| `tags` | map(string) | default `{}` |

## Plan

Subnets are allocated from `vpc_cidr` in order -- each tier in the order
given, and within a tier each zone in the order given -- packed as
Terraform's `cidrsubnets` packs them (each block aligned to its own size).

## Outputs

- `subnets`: a map keyed `"<tier>-<zone>"` to `{ cidr, zone, tier, public,
  name, tags }`, where `name` is `net-<environment>-<tier>-<zone>` and `tags`
  are the given tags plus `Environment` and `Tier` (these two win over given
  tags of the same name).
- `public_subnets`: the names of the public subnets, sorted.
- `capacity`: a map from tier name to the usable addresses of its subnets
  together, counting 5 reserved addresses per subnet (so a /24 has 251).

Run the tests with `terraform init && terraform test`.
