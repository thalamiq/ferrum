import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Use standalone output for Docker builds, standard output for Vercel
  output: process.env.STANDALONE ? "standalone" : undefined,
  devIndicators: false,
};

export default nextConfig;
