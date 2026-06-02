export function formatInteger(value: number, locale = "zh-CN") {
  return new Intl.NumberFormat(locale, {
    maximumFractionDigits: 0,
  }).format(value);
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
