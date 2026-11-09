import { describe, expect, it } from "vitest";
import { formatSi, linearTicks, logTicks, nearestIndex, toCsv, toDb } from "./format";

describe("formatSi", () => {
  it("picks the prefix", () => {
    expect(formatSi(0.0047, "A", "en-US")).toBe("4.7 mA");
    expect(formatSi(1500, "Hz", "en-US")).toBe("1.5 kHz");
    expect(formatSi(2.5e-7, "s", "en-US")).toBe("250 ns");
    expect(formatSi(5, "V", "en-US")).toBe("5 V");
    expect(formatSi(-12, "V", "en-US")).toBe("-12 V");
  });

  it("does not show numerical noise as a tiny value", () => {
    expect(formatSi(1e-19, "V", "en-US")).toBe("0 V");
    expect(formatSi(0, "V", "en-US")).toBe("0 V");
  });

  it("uses the decimal mark of the language", () => {
    expect(formatSi(1.5, "V", "de-DE")).toBe("1,5 V");
  });

  it("names values that are not numbers", () => {
    expect(formatSi(Number.NaN, "V", "en-US")).toBe("NaN V");
  });

  it("does not leave a value of 1000 milli when rounding is close", () => {
    expect(formatSi(1, "V", "en-US")).toBe("1 V");
    expect(formatSi(0.001, "V", "en-US")).toBe("1 mV");
  });
});

describe("toCsv", () => {
  it("writes a header and one row per point with dot decimals", () => {
    const csv = toCsv({
      x_unit: "s",
      x: [0, 0.5],
      traces: [
        { name: "V(out)", unit: "V", values: [1.5, 2] },
        { name: "I(R1)", unit: "A", values: [0.001, 0] },
      ],
    });
    expect(csv).toBe("x [s],V(out) [V],I(R1) [A]\n0,1.5,0.001\n0.5,2,0\n");
  });

  it("quotes names holding a comma", () => {
    const csv = toCsv({ x_unit: "V", x: [1], traces: [{ name: 'a,"b"', unit: "V", values: [1] }] });
    expect(csv.split("\n")[0]).toBe('x [V],"a,""b"" [V]"');
  });
});

describe("axes", () => {
  it("gives round linear ticks", () => {
    expect(linearTicks(0, 5)).toEqual([0, 1, 2, 3, 4, 5]);
    expect(linearTicks(-1, 1)[0]).toBe(-1);
    expect(linearTicks(-1, 1)).toContain(0);
    expect(linearTicks(3, 3)).toEqual([3]);
  });

  it("gives decades", () => {
    expect(logTicks(10, 1e4)).toEqual([10, 100, 1000, 10000]);
    expect(logTicks(15, 900)).toEqual([100]);
  });

  it("finds the nearest sample", () => {
    expect(nearestIndex([0, 1, 2, 10], 1.4)).toBe(1);
    expect(nearestIndex([0, 1, 2, 10], 7)).toBe(3);
    expect(nearestIndex([0, 1, 2, 10], -5)).toBe(0);
    expect(nearestIndex([5], 7)).toBe(0);
  });

  it("converts to decibels", () => {
    expect(toDb(1)).toBe(0);
    expect(toDb(0.1)).toBeCloseTo(-20);
    expect(Number.isFinite(toDb(0))).toBe(true);
  });
});
