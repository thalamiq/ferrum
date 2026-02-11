import { redirect } from "next/navigation";
import { config } from "@/lib/config";

const SearchPage = () => {
  redirect(config.nav.search.subItems.searchParameters.path);
};

export default SearchPage;
