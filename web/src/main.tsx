import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { TooltipProvider } from "@/components/ui/tooltip";
import App from "./App";
import "./index.css";

const client = new QueryClient();
const root = document.getElementById("root");
if (!root) throw new Error("Missing app root");
createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={client}>
      <TooltipProvider delay={250}>
        <App />
      </TooltipProvider>
    </QueryClientProvider>
  </StrictMode>,
);
