import { redirect } from "@sveltejs/kit";

export function load({ url }: { url: URL }) {
  if (url.searchParams.get("mode") !== "work") return {};

  const workUrl = new URL("/chat/work", url);
  for (const [key, value] of url.searchParams) {
    if (key !== "mode") workUrl.searchParams.set(key, value);
  }
  throw redirect(307, `${workUrl.pathname}${workUrl.search}`);
}
