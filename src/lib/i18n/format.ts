import { IntlMessageFormat } from "intl-messageformat";

const pluralMessages = new Map<string, IntlMessageFormat>();

/** 普通占位符保持原契约；ICU 复数交给标准运行时，参数始终只作为数据。 */
export function format(template: string, params: Record<string, string | number>, locale = "en"): string {
  const simple = () => template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
  const pluralNames = [...template.matchAll(/\{(\w+),\s*(?:plural|selectordinal),/g)].map(match => match[1]);
  if (!pluralNames.length) return simple();
  if (pluralNames.some(name => !(name in params) || !Number.isFinite(Number(params[name])))) return template;
  const cacheKey = `${locale}\0${template}`;
  let message = pluralMessages.get(cacheKey);
  if (!message) {
    message = new IntlMessageFormat(template, locale, undefined, { ignoreTag: true });
    pluralMessages.set(cacheKey, message);
  }
  // Missing plain parameters remain visible, as they do in non-ICU messages.
  const values: Record<string, string | number> = { ...params };
  for (const match of template.matchAll(/\{(\w+)(?=\s*[,}])/g)) {
    values[match[1]] ??= `{${match[1]}}`;
  }
  for (const name of pluralNames) values[name] = Number(params[name]);
  return String(message.format(values));
}
