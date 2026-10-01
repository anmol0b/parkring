import React from "react";
import { colors, fonts } from "../theme";

/** A terminal window. Children are its lines. */
export const Terminal: React.FC<{ title: string; width?: number; children: React.ReactNode }> = ({
  title,
  width = 1500,
  children,
}) => (
  <div
    style={{
      width,
      background: colors.panel,
      border: `1px solid ${colors.border}`,
      borderRadius: 16,
      overflow: "hidden",
      boxShadow: "0 30px 80px rgba(0,0,0,0.5)",
    }}
  >
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "16px 22px",
        borderBottom: `1px solid ${colors.border}`,
      }}
    >
      {["#ff5f57", "#febc2e", "#28c840"].map((c) => (
        <div key={c} style={{ width: 14, height: 14, borderRadius: 7, background: c }} />
      ))}
      <div style={{ marginLeft: 16, fontFamily: fonts.mono, fontSize: 22, color: colors.muted }}>{title}</div>
    </div>
    <div style={{ padding: "26px 32px", fontFamily: fonts.mono, fontSize: 32, lineHeight: 1.5, fontVariantLigatures: "none" }}>{children}</div>
  </div>
);
