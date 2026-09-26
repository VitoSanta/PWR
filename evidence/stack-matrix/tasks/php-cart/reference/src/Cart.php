<?php

declare(strict_types=1);

namespace Shop;

use InvalidArgumentException;

final class Cart
{
    private const COUPONS = ['SAVE10', 'FIVEOFF', 'FREESHIP'];

    /** @var array<string, int> */
    private array $quantities = [];
    private ?string $coupon = null;

    public function __construct(private Catalog $catalog)
    {
    }

    public function add(string $sku, int $qty = 1): void
    {
        $this->catalog->product($sku);
        if ($qty < 1) {
            throw new InvalidArgumentException('quantity must be at least 1');
        }
        $this->quantities[$sku] = ($this->quantities[$sku] ?? 0) + $qty;
    }

    public function remove(string $sku, int $qty = 1): void
    {
        if (!isset($this->quantities[$sku]) || $qty < 1) {
            throw new InvalidArgumentException("cannot remove $sku");
        }
        $this->quantities[$sku] -= $qty;
        if ($this->quantities[$sku] <= 0) {
            unset($this->quantities[$sku]);
        }
    }

    /** @return list<array{sku: string, qty: int, unit: int, total: int}> */
    public function lines(): array
    {
        $lines = [];
        foreach ($this->quantities as $sku => $qty) {
            $unit = $this->catalog->product($sku)['price'];
            $lines[] = ['sku' => $sku, 'qty' => $qty, 'unit' => $unit, 'total' => $qty * $unit];
        }
        return $lines;
    }

    public function applyCoupon(string $code): void
    {
        $code = strtoupper($code);
        if (!in_array($code, self::COUPONS, true)) {
            throw new InvalidArgumentException("unknown coupon $code");
        }
        $this->coupon = $code;
    }

    public function removeCoupon(): void
    {
        $this->coupon = null;
    }

    /** @return array{subtotal: int, discount: int, shipping: int, tax: int, total: int} */
    public function totals(): array
    {
        $subtotal = 0;
        $promotion = 0;
        $weight = 0;
        foreach ($this->quantities as $sku => $qty) {
            $product = $this->catalog->product($sku);
            $subtotal += $qty * $product['price'];
            $weight += $qty * $product['weight'];
            if ($product['category'] === 'snacks') {
                $promotion += intdiv($qty, 3) * $product['price'];
            }
        }
        $afterPromotion = $subtotal - $promotion;
        $coupon = match ($this->coupon) {
            'SAVE10' => (int) round($afterPromotion * 0.10, 0, PHP_ROUND_HALF_UP),
            'FIVEOFF' => $afterPromotion >= 2500 ? 500 : 0,
            default => 0,
        };
        $discount = $promotion + $coupon;
        if ($weight === 0 || $this->coupon === 'FREESHIP' || $subtotal - $discount >= 5000) {
            $shipping = 0;
        } elseif ($weight <= 1000) {
            $shipping = 490;
        } elseif ($weight <= 5000) {
            $shipping = 890;
        } else {
            $shipping = 890 + 150 * intdiv($weight - 5000 + 999, 1000);
        }
        $tax = (int) round(($subtotal - $discount + $shipping) * 0.22, 0, PHP_ROUND_HALF_UP);
        return [
            'subtotal' => $subtotal,
            'discount' => $discount,
            'shipping' => $shipping,
            'tax' => $tax,
            'total' => $subtotal - $discount + $shipping + $tax,
        ];
    }
}
