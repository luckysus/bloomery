import { useId, type SVGProps } from "react";

/** The single Suna brand mark used by the shell, welcome view and desktop bundle. */
export default function SunaLogo({ size = 40, title = "Suna", ...props }: SVGProps<SVGSVGElement> & { size?: number; title?: string }) {
  const gradientId = useId().replace(/:/g, "");
  return (
    <svg
      {...props}
      className={`suna-logo${props.className ? ` ${props.className}` : ""}`}
      width={size}
      height={size}
      viewBox="0 0 80 80"
      fill="none"
      role="img"
      aria-label={title}
    >
      <title>{title}</title>
      <defs>
        <linearGradient id={`${gradientId}-top`} x1="12" y1="8" x2="67" y2="34" gradientUnits="userSpaceOnUse"><stop stopColor="#2D65F1" /><stop offset=".52" stopColor="#35A8F7" /><stop offset="1" stopColor="#1689E8" /></linearGradient>
        <linearGradient id={`${gradientId}-left`} x1="12" y1="18" x2="40" y2="42" gradientUnits="userSpaceOnUse"><stop stopColor="#2C64E9" /><stop offset="1" stopColor="#579CF3" /></linearGradient>
        <linearGradient id={`${gradientId}-right`} x1="40" y1="39" x2="69" y2="22" gradientUnits="userSpaceOnUse"><stop stopColor="#1C43D1" /><stop offset="1" stopColor="#1A73EA" /></linearGradient>
      </defs>
      <g>
        <path d="M40 4 68 20 54 28 40 20 26 28 12 20 40 4Z" fill={`url(#${gradientId}-top)`} />
        <path d="M12 20 26 28 40 36v14L12 34V20Z" fill={`url(#${gradientId}-left)`} />
        <path d="M68 20 54 28 40 36v14l28-16V20Z" fill={`url(#${gradientId}-right)`} />
      </g>
      <g>
        <path d="M40 30 68 46 54 54 40 46 26 54 12 46 40 30Z" fill={`url(#${gradientId}-top)`} />
        <path d="M12 46 26 54 40 62v14L12 60V46Z" fill={`url(#${gradientId}-left)`} />
        <path d="M68 46 54 54 40 62v14l28-16V46Z" fill={`url(#${gradientId}-right)`} />
      </g>
    </svg>
  );
}
