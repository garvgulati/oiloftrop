export function usChartUrl(exchange: string, ticker: string): string | null {
  if (
    !["NASDAQ", "NYSE", "AMEX", "NYSEARCA"].includes(exchange) ||
    !/^[A-Z][A-Z0-9.-]{0,19}$/.test(ticker)
  )
    return null;
  const url = new URL("https://www.tradingview-widget.com/embed-widget/advanced-chart/");
  url.searchParams.set("locale", "en");
  url.hash = encodeURIComponent(
    JSON.stringify({
      symbol: `${exchange}:${ticker}`,
      interval: "D",
      timezone: "America/New_York",
      theme: "light",
      style: "1",
      locale: "en",
      allow_symbol_change: false,
      hide_side_toolbar: true,
      hide_top_toolbar: false,
      withdateranges: true,
      autosize: true,
      save_image: false,
      calendar: false,
      details: false,
      hotlist: false,
    }),
  );
  return url.toString();
}
