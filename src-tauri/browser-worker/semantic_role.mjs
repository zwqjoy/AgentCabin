export function semanticRole(tag, type = "", role = "") {
  if (role) return role;
  if (tag === "a") return "link";
  if (tag === "input") {
    if (type === "checkbox" || type === "radio") return type;
    if (type === "submit" || type === "button" || type === "reset") return "button";
    if (type === "search") return "searchbox";
    return "textbox";
  }
  return tag;
}
