export function formatInteger(value: number, locale = "zh-CN") {
  return new Intl.NumberFormat(locale, {
    maximumFractionDigits: 0,
  }).format(value);
}

export function formatCompactNumber(value: number, locale = "zh-CN") {
  if (!Number.isFinite(value)) {
    return formatInteger(0, locale);
  }

  const absValue = Math.abs(value);
  const unit =
    absValue >= 1_000_000_000
      ? { divisor: 1_000_000_000, suffix: "B" }
      : absValue >= 1_000_000
        ? { divisor: 1_000_000, suffix: "M" }
        : absValue >= 1_000
          ? { divisor: 1_000, suffix: "K" }
          : null;

  if (!unit) {
    return formatInteger(value, locale);
  }

  const scaledValue = absValue / unit.divisor;
  const maximumFractionDigits = scaledValue >= 100 ? 0 : scaledValue >= 10 ? 1 : 2;
  const formattedValue = new Intl.NumberFormat(locale, {
    maximumFractionDigits,
    minimumFractionDigits: 0,
  }).format(scaledValue);

  return `${value < 0 ? "-" : ""}${formattedValue}${unit.suffix}`;
}

export function formatCompactToken(value: number, locale = "zh-CN") {
  return formatCompactNumber(value, locale);
}

export function formatBytes(value: number, locale = "zh-CN") {
  if (!Number.isFinite(value) || value <= 0) {
    return `0 B`;
  }

  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }

  const maximumFractionDigits = size >= 100 || unitIndex === 0 ? 0 : size >= 10 ? 1 : 2;
  const formattedSize = new Intl.NumberFormat(locale, {
    maximumFractionDigits,
    minimumFractionDigits: 0,
  }).format(size);

  return `${formattedSize} ${units[unitIndex]}`;
}

export function formatTokenByDisplayMode(
  value: number,
  locale = "zh-CN",
  mode: "compact" | "full" = "compact",
) {
  return mode === "full" ? formatInteger(value, locale) : formatCompactToken(value, locale);
}

export function formatCost(value: number, currency = "USD") {
  const normalizedCurrency = currency.trim().toUpperCase();
  if (normalizedCurrency === "MIXED") {
    return `多币种 ${new Intl.NumberFormat("zh-CN", {
      minimumFractionDigits: value > 0 && value < 0.01 ? 4 : 2,
      maximumFractionDigits: 4,
    }).format(value)}`;
  }

  const locale = normalizedCurrency === "CNY" ? "zh-CN" : "en-US";
  const safeCurrency = normalizedCurrency === "CNY" ? "CNY" : "USD";
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: safeCurrency,
    minimumFractionDigits: value > 0 && value < 0.01 ? 4 : 2,
    maximumFractionDigits: 4,
  }).format(value);
}

export function formatCurrencyName(currency: string) {
  switch (currency.trim().toUpperCase()) {
    case "CNY":
      return "人民币";
    case "MIXED":
      return "多币种";
    case "USD":
    default:
      return "美元";
  }
}

export function formatPercent(value: number, locale = "zh-CN") {
  return new Intl.NumberFormat(locale, {
    style: "percent",
    maximumFractionDigits: 1,
  }).format(value);
}

export function formatLatency(value: number | null, emptyLabel = "无") {
  if (value === null) {
    return emptyLabel;
  }

  return `${Math.round(value)} ms`;
}

export function formatDateTime(value: string | null, emptyLabel = "无") {
  if (!value) {
    return emptyLabel;
  }

  return value.replace("T", " ").slice(0, 19);
}
