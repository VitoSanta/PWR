<?php

declare(strict_types=1);

namespace Shop;

use InvalidArgumentException;

final class Catalog
{
    /** @param array<string, array{price: int, category: string, weight: int}> $products */
    public function __construct(private array $products)
    {
    }

    /** @return array{price: int, category: string, weight: int} */
    public function product(string $sku): array
    {
        if (!isset($this->products[$sku])) {
            throw new InvalidArgumentException("unknown sku $sku");
        }
        return $this->products[$sku];
    }
}
