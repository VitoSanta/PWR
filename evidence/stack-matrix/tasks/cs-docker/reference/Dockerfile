FROM mcr.microsoft.com/dotnet/sdk:10.0 AS build
WORKDIR /src
COPY src/Inventory.Api/Inventory.Api.csproj src/Inventory.Api/
RUN dotnet restore src/Inventory.Api/Inventory.Api.csproj
COPY src/ src/
RUN dotnet publish src/Inventory.Api/Inventory.Api.csproj -c Release -o /app --no-restore

FROM mcr.microsoft.com/dotnet/aspnet:10.0
WORKDIR /app
COPY --from=build /app .
ENV ASPNETCORE_URLS=http://+:8080
EXPOSE 8080
USER app
ENTRYPOINT ["dotnet", "Inventory.Api.dll"]
